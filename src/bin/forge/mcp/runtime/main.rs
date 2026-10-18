//! An MCP server that exposes the operations of an API as tools, and calls
//! the API over HTTP when a client calls a tool.
//!
//! The server speaks MCP over standard input and output. It reads the base
//! URL of the API from `API_BASE_URL`, which defaults to the first server of
//! the `OpenAPI` document, and sends `API_TOKEN`, when it is set, as a bearer
//! token.

use std::borrow::Cow;
use std::fmt::Write;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, JsonObject, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;

mod tools;

/// An operation of the API, exposed as a tool.
struct Tool {
    /// The name of the tool, unique within the server.
    name: &'static str,
    /// A short, human-readable title, or an empty string.
    title: &'static str,
    /// What the tool does, or an empty string.
    description: &'static str,
    /// The HTTP method the operation is sent with, such as `GET`.
    method: &'static str,
    /// The path template of the operation, such as `/accounts/{account_id}`.
    path: &'static str,
    /// The parameters of the operation, which are sent in the path or query.
    params: &'static [Param],
    /// Where the request body comes from.
    body: Body,
    /// The JSON Schema of the arguments of the tool.
    input_schema: &'static str,
    /// Whether the operation changes nothing.
    read_only: bool,
    /// Whether the operation can change or delete something irreversibly.
    destructive: bool,
    /// Whether calling the operation twice has the effect of calling it once.
    idempotent: bool,
}

/// A path or query parameter of an operation.
struct Param {
    /// The name of the argument, and of the parameter on the wire.
    name: &'static str,
    location: Location,
}

/// Where a parameter is sent.
enum Location {
    /// In the path, in place of `{name}`.
    Path,
    /// In the query. An exploded array is sent as one pair per element; any
    /// other array is sent as one pair, its elements separated by commas.
    Query { exploded: bool },
}

/// Where the JSON request body of an operation comes from.
enum Body {
    /// The operation has no body.
    None,
    /// The body is the arguments, without the parameters.
    Flat,
    /// The body is the argument `body`.
    Field,
}

/// Serves the tools of the API.
#[derive(Clone)]
struct Server {
    http: reqwest::Client,
    base_url: Arc<str>,
    token: Option<Arc<str>>,
}

impl Server {
    /// Returns the server, configured from the environment.
    fn from_env() -> Server {
        let base_url = std::env::var("API_BASE_URL").unwrap_or_else(|_| tools::BASE_URL.to_owned());
        let token = std::env::var("API_TOKEN").ok().filter(|token| !token.is_empty());
        Server {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').into(),
            token: token.map(Into::into),
        }
    }

    /// Calls the operation of `tool` with `args`, and returns the response
    /// body as the result. A response with an error status is a result with
    /// an error, so that the client sees it.
    async fn call(&self, tool: &Tool, args: JsonObject) -> CallToolResult {
        let request = match self.request(tool, args) {
            Ok(request) => request,
            Err(err) => return CallToolResult::error(vec![ContentBlock::text(err)]),
        };
        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => return CallToolResult::error(vec![ContentBlock::text(format!("request failed: {err}"))]),
        };
        let status = response.status();
        let text = match response.text().await {
            Ok(text) => text,
            Err(err) => {
                return CallToolResult::error(vec![ContentBlock::text(format!("reading the response failed: {err}"))]);
            }
        };
        if status.is_success() {
            CallToolResult::success(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::error(vec![ContentBlock::text(format!("{status}\n\n{text}"))])
        }
    }

    /// Returns the HTTP request that calls the operation of `tool` with
    /// `args`.
    fn request(&self, tool: &Tool, mut args: JsonObject) -> Result<reqwest::RequestBuilder, String> {
        let mut path = tool.path.to_owned();
        let mut query = Vec::new();
        for param in tool.params {
            let Some(value) = args.remove(param.name) else {
                if matches!(param.location, Location::Path) {
                    return Err(format!("missing argument `{}`", param.name));
                }
                continue;
            };
            match param.location {
                Location::Path => {
                    let value = scalar(&value).ok_or_else(|| format!("`{}` must be a string or number", param.name))?;
                    path = path.replace(&format!("{{{}}}", param.name), &encode(&value));
                }
                Location::Query { exploded } => match value {
                    Value::Null => {}
                    Value::Array(values) if exploded => {
                        query.extend(values.iter().filter_map(scalar).map(|v| (param.name, v)));
                    }
                    Value::Array(values) => {
                        let values: Vec<String> = values.iter().filter_map(scalar).collect();
                        query.push((param.name, values.join(",")));
                    }
                    value => {
                        let value = scalar(&value).ok_or_else(|| format!("`{}` must not be an object", param.name))?;
                        query.push((param.name, value));
                    }
                },
            }
        }

        let method = reqwest::Method::from_bytes(tool.method.as_bytes()).map_err(|err| err.to_string())?;
        let mut url = reqwest::Url::parse(&format!("{}{path}", self.base_url)).map_err(|err| err.to_string())?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let mut request = self.http.request(method, url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let body = match tool.body {
            Body::None => None,
            Body::Flat => Some(Value::Object(args)),
            Body::Field => args.remove("body"),
        };
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
        }
        Ok(request)
    }
}

/// Returns `value` as it is written in a path or query, or `None` if it is
/// an array or object.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Returns `s` percent-encoded as a path segment.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Returns the MCP definition of `tool`.
fn definition(tool: &Tool) -> Result<rmcp::model::Tool, serde_json::Error> {
    let schema: JsonObject = serde_json::from_str(tool.input_schema)?;
    let description = (!tool.description.is_empty()).then_some(Cow::Borrowed(tool.description));
    let mut annotations = ToolAnnotations::new()
        .read_only(tool.read_only)
        .destructive(tool.destructive)
        .idempotent(tool.idempotent)
        .open_world(true);
    if !tool.title.is_empty() {
        annotations.title = Some(tool.title.to_owned());
    }
    let mut definition = rmcp::model::Tool::new_with_raw(tool.name, description, schema).with_annotations(annotations);
    if !tool.title.is_empty() {
        definition = definition.with_title(tool.title);
    }
    Ok(definition)
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")));
        if !tools::INSTRUCTIONS.is_empty() {
            config = config.with_instructions(tools::INSTRUCTIONS);
        }
        config
    }

    fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> {
        let tools = tools::TOOLS
            .iter()
            .map(definition)
            .collect::<Result<Vec<_>, _>>()
            .map(ListToolsResult::with_all_items)
            .map_err(|err| ErrorData::internal_error(err.to_string(), None));
        std::future::ready(tools)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(tool) = tools::TOOLS.iter().find(|tool| tool.name == request.name) else {
            return Err(ErrorData::invalid_params(
                format!("unknown tool `{}`", request.name),
                None,
            ));
        };
        Ok(self.call(tool, request.arguments.unwrap_or_default()).await.into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::from_env().serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
