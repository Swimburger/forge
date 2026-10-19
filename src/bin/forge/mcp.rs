//! `forge mcp`, which serves the methods of an OpenAPI document as the tools
//! of an [MCP](https://modelcontextprotocol.io) server, and calls the API
//! over HTTP when a client calls one.
//!
//! ```text
//! forge mcp openapi.json
//! forge mcp openapi.json --listen 127.0.0.1:9000 --group dns.records
//! forge mcp openapi.json --transport stdio
//! ```
//!
//! The server speaks MCP with [rmcp](https://crates.io/crates/rmcp), the
//! official Rust SDK, over Streamable HTTP at `/mcp` or over standard input
//! and output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::ValueEnum;
use forge::{Err, Forge};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};

mod request;
mod schema;
mod tools;

use request::{HttpRequest, HttpResponse};
use tools::{Api, Tool};

/// The path the HTTP transport serves MCP at.
const HTTP_PATH: &str = "/mcp";

/// Serves the methods of an OpenAPI document as the tools of an MCP server.
///
/// It calls the API at --base-url, and sends the API_TOKEN environment
/// variable, when it is set, as a bearer token. Over HTTP, a client's own
/// Authorization header is sent instead, when it has one.
#[derive(clap::Args)]
pub struct Args {
    /// The OpenAPI JSON document, or - to read it from standard input, which
    /// the stdio transport cannot.
    spec: PathBuf,
    /// How clients connect to the server: `http` serves MCP's Streamable
    /// HTTP transport at /mcp on --listen, and `stdio` speaks MCP over
    /// standard input and output, for a client that runs forge itself.
    #[arg(long, value_enum, default_value_t = Transport::Http)]
    transport: Transport,
    /// The address the HTTP transport listens on. Port 0 picks a free port.
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: String,
    /// A host name, or `host:port`, that clients may reach the HTTP
    /// transport by, besides localhost, 127.0.0.1 and ::1, which are always
    /// allowed. Requests for any other host are refused, to guard against
    /// DNS rebinding. Can be given more than once.
    #[arg(long = "allow-host", value_name = "HOST")]
    allowed_hosts: Vec<String>,
    /// The base URL of the API. Defaults to the API_BASE_URL environment
    /// variable, or else the first server in the document.
    #[arg(long)]
    base_url: Option<String>,
    /// Serve only the methods in GROUP, such as `dns.records`, and in the
    /// groups inside it. Can be given more than once.
    #[arg(long = "group", value_name = "GROUP")]
    groups: Vec<String>,
}

/// How clients connect to the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Transport {
    /// MCP's Streamable HTTP transport.
    Http,
    /// Standard input and output.
    Stdio,
}

/// Serves the document of `args`, which `read` reads, until the client
/// disconnects from the stdio transport, or the HTTP transport is stopped
/// with Ctrl-C.
pub fn run(args: Args, read: fn(&Path) -> Result<Forge, Err>) -> Result<(), Err> {
    if args.transport == Transport::Stdio && args.spec == Path::new("-") {
        return Err("the stdio transport reads MCP from standard input, so it cannot read the document there".into());
    }
    let forge = read(&args.spec)?;
    let api = Api::new(&forge, &args.groups).map_err(|err| format!("{}: {err}", args.spec.display()))?;
    drop(forge);
    if !api.skipped.is_empty() {
        // A large document can skip many methods, so only some are named.
        const SHOWN: usize = 20;
        let mut lines = api.skipped[..api.skipped.len().min(SHOWN)].join("\n  ");
        if api.skipped.len() > SHOWN {
            lines.push_str(&format!("\n  and {} more", api.skipped.len() - SHOWN));
        }
        eprintln!("forge: {} methods have no tool:\n  {lines}", api.skipped.len());
    }

    let base_url = args
        .base_url
        .clone()
        .or_else(|| std::env::var("API_BASE_URL").ok())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| api.base_url.clone());
    if base_url.is_empty() {
        return Err("the document has no servers: give the base URL of the API with --base-url".into());
    }
    let token = std::env::var("API_TOKEN").ok().filter(|token| !token.is_empty());
    let server = Server::new(api, base_url, token)?;

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async {
        match args.transport {
            Transport::Http => serve_http(server, &args.listen, &args.allowed_hosts).await,
            Transport::Stdio => serve_stdio(server).await,
        }
    })
}

/// Serves MCP over Streamable HTTP at `/mcp` on `listen`, until Ctrl-C.
async fn serve_http(server: Server, listen: &str, allowed_hosts: &[String]) -> Result<(), Err> {
    let hosts = ["localhost", "127.0.0.1", "::1"]
        .into_iter()
        .map(str::to_owned)
        .chain(allowed_hosts.iter().cloned());
    let config = StreamableHttpServerConfig::default().with_allowed_hosts(hosts);
    let tools = server.tools.tools.len();
    let base_url = server.tools.base_url.clone();
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let router = axum::Router::new().nest_service(HTTP_PATH, service);
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|err| format!("cannot listen on {listen}: {err}"))?;
    let addr = listener.local_addr()?;
    eprintln!("forge: serving {tools} tools for {base_url} at http://{addr}{HTTP_PATH}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            // Without a handler for Ctrl-C, the server runs until killed.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        })
        .await?;
    Ok(())
}

/// Serves MCP over standard input and output, until the client closes
/// standard input.
async fn serve_stdio(server: Server) -> Result<(), Err> {
    eprintln!(
        "forge: serving {} tools for {} over stdio",
        server.tools.tools.len(),
        server.tools.base_url
    );
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// Serves the tools of an [`Api`]. Cloning it is cheap, since the HTTP
/// transport makes one for each session.
#[derive(Clone)]
struct Server {
    tools: Arc<Tools>,
    http: reqwest::Client,
}

/// What every clone of a [`Server`] shares.
struct Tools {
    description: String,
    base_url: String,
    /// The token to send as a bearer token, when the client sends none.
    token: Option<String>,
    tools: Vec<Tool>,
    /// The index of each tool in `tools`, by name.
    by_name: HashMap<String, usize>,
}

impl Server {
    /// Returns the server of `api`, which calls the API at `base_url` and
    /// sends `token` as a bearer token.
    fn new(api: Api, base_url: String, token: Option<String>) -> Result<Server, Err> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("forge/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let by_name = api
            .tools
            .iter()
            .enumerate()
            .map(|(i, t)| (t.definition.name.to_string(), i))
            .collect();
        Ok(Server {
            tools: Arc::new(Tools {
                description: api.description,
                base_url,
                token,
                tools: api.tools,
                by_name,
            }),
            http,
        })
    }

    /// Calls `tool` with `args`, sending `authorization` as the
    /// `Authorization` header, or else the server's token.
    async fn call(
        &self,
        tool: &Tool,
        args: serde_json::Map<String, serde_json::Value>,
        authorization: Option<&str>,
    ) -> CallToolResult {
        let mut request = match HttpRequest::new(&tool.endpoint, &self.tools.base_url, args) {
            Ok(request) => request,
            Err(err) => return CallToolResult::error(vec![ContentBlock::text(err)]),
        };
        if let Some(authorization) = authorization {
            request.authorize(authorization);
        } else if let Some(token) = &self.tools.token {
            request.bearer(token);
        }
        result(send(&self.http, &request).await)
    }
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("forge", env!("CARGO_PKG_VERSION")));
        if !self.tools.description.is_empty() {
            config = config.with_instructions(self.tools.description.clone());
        }
        config
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let definitions = self.tools.tools.iter().map(|t| t.definition.clone()).collect();
        Ok(ListToolsResult::with_all_items(definitions))
    }

    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        let &i = self.tools.by_name.get(name)?;
        Some(self.tools.tools[i].definition.clone())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(&i) = self.tools.by_name.get(request.name.as_ref()) else {
            return Err(ErrorData::invalid_params(
                format!("unknown tool {:?}", request.name),
                None,
            ));
        };
        // Over HTTP, the client's request is in the context.
        let authorization = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.headers.get(http::header::AUTHORIZATION))
            .and_then(|value| value.to_str().ok());
        let args = request.arguments.unwrap_or_default();
        Ok(self.call(&self.tools.tools[i], args, authorization).await.into())
    }
}

/// Sends `request` and returns its response.
async fn send(http: &reqwest::Client, request: &HttpRequest) -> Result<HttpResponse, String> {
    let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|err| err.to_string())?;
    let mut builder = http.request(method, request.url.as_str());
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    if let Some(body) = &request.body {
        builder = builder.body(body.clone());
    }
    let response = builder.send().await.map_err(|err| chain(&err))?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response.bytes().await.map_err(|err| chain(&err))?.to_vec();
    Ok(HttpResponse {
        status,
        content_type,
        body,
    })
}

/// Returns the result of a tool, given the response to its request, or
/// why there is none. The result is an error when the request failed or the
/// response has an error status, so the client sees why.
fn result(response: Result<HttpResponse, String>) -> CallToolResult {
    let response = match response {
        Ok(response) => response,
        Err(err) => return CallToolResult::error(vec![ContentBlock::text(format!("the request failed: {err}"))]),
    };
    let text = match String::from_utf8(response.body) {
        Ok(text) => text,
        Err(err) => format!(
            "a response of {} bytes of {}",
            err.as_bytes().len(),
            response.content_type.as_deref().unwrap_or("binary data"),
        ),
    };
    if (200..300).contains(&response.status) {
        CallToolResult::success(vec![ContentBlock::text(text)])
    } else {
        CallToolResult::error(vec![ContentBlock::text(format!("HTTP {}\n\n{text}", response.status))])
    }
}

/// Returns `err` and the errors that caused it, separated by colons, since
/// reqwest's errors leave out their causes, such as a refused connection.
fn chain(err: &dyn std::error::Error) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(err) = source {
        message.push_str(": ");
        message.push_str(&err.to_string());
        source = err.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{HttpResponse, result};

    fn text(result: &rmcp::model::CallToolResult) -> Value {
        serde_json::to_value(result).unwrap()
    }

    #[test]
    fn results() {
        let ok = result(Ok(HttpResponse {
            status: 200,
            content_type: Some("application/json".to_owned()),
            body: br#"{"id":1}"#.to_vec(),
        }));
        assert_eq!(text(&ok)["content"], json!([{"type": "text", "text": r#"{"id":1}"#}]));
        assert_eq!(ok.is_error, Some(false));

        let not_found = result(Ok(HttpResponse {
            status: 404,
            content_type: None,
            body: b"no such item".to_vec(),
        }));
        assert_eq!(
            text(&not_found)["content"],
            json!([{"type": "text", "text": "HTTP 404\n\nno such item"}])
        );
        assert_eq!(not_found.is_error, Some(true));

        let binary = result(Ok(HttpResponse {
            status: 200,
            content_type: Some("application/pdf".to_owned()),
            body: vec![0xff, 0xfe],
        }));
        assert_eq!(
            text(&binary)["content"][0]["text"],
            "a response of 2 bytes of application/pdf"
        );

        let failed = result(Err("connection refused".to_owned()));
        assert_eq!(
            text(&failed)["content"][0]["text"],
            "the request failed: connection refused"
        );
        assert_eq!(failed.is_error, Some(true));
    }
}
