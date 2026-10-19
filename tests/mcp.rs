//! Runs `forge mcp` against an API served on a local port, over both
//! transports.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;

use serde_json::{Value, json};

/// A document with one method, `items.update`.
const SPEC: &str = r#"{
  "openapi": "3.1.0", "info": {"title": "Items", "version": "1"},
  "servers": [{"url": "https://unused.example.com"}],
  "paths": {"/items/{id}": {"patch": {
    "x-fern-sdk-group-name": "items", "x-fern-sdk-method-name": "update",
    "summary": "Update an item",
    "parameters": [
      {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}},
      {"name": "dry_run", "in": "query", "schema": {"type": "boolean"}}
    ],
    "requestBody": {"required": true, "content": {"application/json": {"schema": {
      "type": "object", "properties": {"name": {"type": "string"}}}}}},
    "responses": {"200": {"description": "ok"}}}}}
}"#;

/// What the API answers with.
const ITEM: &str = r#"{"id":"1","name":"new"}"#;

/// A file that is removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    /// Writes `content` to a file named after `name` and this process.
    fn new(name: &str, content: &str) -> TempFile {
        let path = std::env::temp_dir().join(format!("forge-mcp-{name}-{}.json", std::process::id()));
        std::fs::write(&path, content).unwrap();
        TempFile(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A child process that is killed when dropped.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts `forge` with `args`, with `API_TOKEN` set to `server-token`.
fn forge(args: &[&str]) -> Running {
    let child = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(args)
        .env_remove("API_BASE_URL")
        .env("API_TOKEN", "server-token")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    Running(child)
}

/// Serves one HTTP request on a local port, answering it with [`ITEM`],
/// and returns the base URL and the text of the request it receives.
fn serve_once() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap();
            }
            request.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let mut content = vec![0; length];
        reader.read_exact(&mut content).unwrap();
        request.push_str(&String::from_utf8(content).unwrap());
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ITEM}",
            ITEM.len()
        )
        .unwrap();
        tx.send(request).unwrap();
    });
    (url, rx)
}

/// Checks the request the API received for `items_update`, sent with
/// `authorization`.
#[track_caller]
fn check_request(request: &str, authorization: &str) {
    assert!(
        request.starts_with("PATCH /items/1?dry_run=true HTTP/1.1\r\n"),
        "{request}"
    );
    let lower = request.to_ascii_lowercase();
    assert!(
        lower.contains(&format!(
            "\r\nauthorization: {}\r\n",
            authorization.to_ascii_lowercase()
        )),
        "{request}"
    );
    assert!(lower.contains("\r\ncontent-type: application/json\r\n"), "{request}");
    assert!(request.ends_with("\r\n\r\n{\"name\":\"new\"}"), "{request}");
}

/// The message that calls `items_update`, as request `id`.
fn call(id: u64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": "items_update", "arguments": {"id": "1", "dry_run": true, "name": "new"}}})
}

/// The message that initializes the session, as request 1.
fn initialize() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
           "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                      "clientInfo": {"name": "test", "version": "1"}}})
}

/// Checks the answers to `initialize`, `tools/list` and `tools/call`.
#[track_caller]
fn check_replies(init: &Value, list: &Value, called: &Value) {
    let init = &init["result"];
    assert_eq!(init["protocolVersion"], "2025-06-18", "{init}");
    assert_eq!(init["serverInfo"]["name"], "forge", "{init}");
    assert_eq!(init["instructions"], "Items", "{init}");

    let tools = &list["result"]["tools"];
    assert_eq!(tools.as_array().map(Vec::len), Some(1), "{list}");
    assert_eq!(tools[0]["name"], "items_update", "{list}");
    assert_eq!(tools[0]["title"], "Update an item", "{list}");

    let result = &called["result"];
    assert_eq!(result["content"], json!([{"type": "text", "text": ITEM}]), "{called}");
    assert_eq!(result["isError"], json!(false), "{called}");
}

/// Reads messages from `stdout` until it has the answers to `ids`.
fn read_replies(stdout: &mut BufReader<ChildStdout>, ids: &[u64]) -> HashMap<u64, Value> {
    let mut replies = HashMap::new();
    while ids.iter().any(|id| !replies.contains_key(id)) {
        let mut line = String::new();
        assert!(stdout.read_line(&mut line).unwrap() > 0, "standard output ended");
        let message: Value = serde_json::from_str(&line).unwrap();
        if let Some(id) = message["id"].as_u64() {
            replies.insert(id, message);
        }
    }
    replies
}

#[test]
fn stdio() {
    let (url, requests) = serve_once();
    let spec = TempFile::new("stdio", SPEC);
    let mut forge = forge(&["mcp", spec.path(), "--transport", "stdio", "--base-url", &url]);
    let mut stdin = forge.0.stdin.take().unwrap();
    let mut stdout = BufReader::new(forge.0.stdout.take().unwrap());

    writeln!(stdin, "{}", initialize()).unwrap();
    let init = read_replies(&mut stdout, &[1]).remove(&1).unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    )
    .unwrap();
    writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    writeln!(stdin, "{}", call(3)).unwrap();
    let mut replies = read_replies(&mut stdout, &[2, 3]);
    check_replies(&init, &replies.remove(&2).unwrap(), &replies.remove(&3).unwrap());
    check_request(&requests.recv().unwrap(), "Bearer server-token");

    // The server stops when the client closes standard input.
    drop(stdin);
    let status = forge.0.wait().unwrap();
    assert!(status.success(), "{status}");
}

/// Returns the URL that `forge` serves MCP at, from what it prints.
fn http_url(forge: &mut Running) -> String {
    let stderr = BufReader::new(forge.0.stderr.take().unwrap());
    for line in stderr.lines() {
        let line = line.unwrap();
        if let Some((_, url)) = line.split_once(" at ") {
            return url.to_owned();
        }
    }
    panic!("forge did not say where it serves MCP");
}

/// Posts `message` to the MCP server at `url`, in `session`, and returns
/// the response's session ID and the message it answers with, if any.
async fn post(
    http: &reqwest::Client,
    url: &str,
    session: Option<&str>,
    message: &Value,
) -> (Option<String>, Option<Value>) {
    let mut request = http
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("MCP-Protocol-Version", "2025-06-18")
        .header("Authorization", "Bearer client-token")
        .body(message.to_string());
    if let Some(session) = session {
        request = request.header("Mcp-Session-Id", session);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let session = response
        .headers()
        .get("mcp-session-id")
        .map(|id| id.to_str().unwrap().to_owned());
    let event_stream = response
        .headers()
        .get("content-type")
        .is_some_and(|ty| ty.to_str().unwrap().starts_with("text/event-stream"));
    let body = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {body}");
    // The answer is either JSON, or the data of an event in a stream.
    let reply = if event_stream {
        body.lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .find(|data| !data.is_empty())
            .map(|data| serde_json::from_str(data).unwrap())
    } else if body.is_empty() {
        None
    } else {
        Some(serde_json::from_str(&body).unwrap())
    };
    (session, reply)
}

#[tokio::test]
async fn http() {
    let (url, requests) = serve_once();
    let spec = TempFile::new("http", SPEC);
    let mut forge = forge(&["mcp", spec.path(), "--listen", "127.0.0.1:0", "--base-url", &url]);
    let mcp = http_url(&mut forge);
    assert!(mcp.starts_with("http://127.0.0.1:") && mcp.ends_with("/mcp"), "{mcp}");

    let http = reqwest::Client::new();
    let (session, init) = post(&http, &mcp, None, &initialize()).await;
    let session = session.expect("no session ID");
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let (_, none) = post(&http, &mcp, Some(&session), &initialized).await;
    assert_eq!(none, None);
    let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
    let (_, list) = post(&http, &mcp, Some(&session), &list).await;
    let (_, called) = post(&http, &mcp, Some(&session), &call(3)).await;
    check_replies(&init.unwrap(), &list.unwrap(), &called.unwrap());
    // The client's own Authorization header wins over API_TOKEN.
    check_request(&requests.recv().unwrap(), "Bearer client-token");
}

#[test]
fn http_is_the_default() {
    let spec = TempFile::new("default", SPEC);
    let mut forge = forge(&["mcp", spec.path(), "--listen", "127.0.0.1:0"]);
    let url = http_url(&mut forge);
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
}

#[test]
fn stdio_cannot_read_the_document_from_stdin() {
    let out = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["mcp", "-", "--transport", "stdio"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("cannot read the document there"), "{err}");
}

#[test]
fn unknown_group() {
    let spec = TempFile::new("group", SPEC);
    let out = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["mcp", spec.path(), "--group", "nope"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains(r#"there is no group "nope""#), "{err}");
}

#[test]
fn needs_a_base_url() {
    let spec = TempFile::new(
        "base-url",
        r#"{"openapi": "3.1.0", "info": {"title": "t", "version": "1"}, "paths": {}}"#,
    );
    let out = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["mcp", spec.path()])
        .env_remove("API_BASE_URL")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("--base-url"), "{err}");
}
