//! The HTTP requests that call tools.

use std::fmt::Write;

use serde_json::{Map, Value};

use super::tools::{Body, Endpoint, Location};

/// An HTTP request to send to call a tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HttpRequest {
    /// The method, such as `GET`.
    pub method: String,
    /// The absolute URL, with its query.
    pub url: String,
    /// The headers, by name, in the order to send them.
    pub headers: Vec<(String, String)>,
    /// The JSON body, if there is one.
    pub body: Option<String>,
}

impl HttpRequest {
    /// Sends `token` as a bearer token, unless the request already has an
    /// `Authorization` header, from an argument of the tool.
    pub(super) fn bearer(&mut self, token: &str) {
        self.authorize(&format!("Bearer {token}"));
    }

    /// Sends `authorization` as the `Authorization` header, unless the
    /// request already has one, from an argument of the tool.
    pub(super) fn authorize(&mut self, authorization: &str) {
        if !self.header("authorization") {
            self.headers
                .push(("Authorization".to_owned(), authorization.to_owned()));
        }
    }

    /// Reports whether the request has the header `name`, ignoring case.
    fn header(&self, name: &str) -> bool {
        self.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// Returns the request that calls `endpoint` with `args`, the
    /// arguments of its tool, against the API at `base_url`, or why `args`
    /// cannot call it.
    pub(super) fn new(
        endpoint: &Endpoint,
        base_url: &str,
        mut args: Map<String, Value>,
    ) -> Result<HttpRequest, String> {
        let mut path = endpoint.path.clone();
        let mut query = String::new();
        let mut headers = Vec::new();
        for param in &endpoint.params {
            let name = param.name.as_str();
            let value = match args.remove(name) {
                None | Some(Value::Null) if param.location == Location::Path => {
                    return Err(format!("missing argument `{name}`"));
                }
                None | Some(Value::Null) => continue,
                Some(value) => value,
            };
            match param.location {
                Location::Path => {
                    let value =
                        scalar(&value).ok_or_else(|| format!("`{name}` must be a string, number or boolean"))?;
                    path = path.replace(&format!("{{{name}}}"), &encode(&value));
                }
                Location::Query { exploded } => {
                    for (key, value) in pairs(name, value, exploded)? {
                        let sep = if query.is_empty() { '?' } else { '&' };
                        write!(query, "{sep}{}={}", encode(&key), encode(&value)).unwrap();
                    }
                }
                Location::Header => {
                    let value = match &value {
                        Value::Array(values) => values
                            .iter()
                            .map(scalar)
                            .collect::<Option<Vec<_>>>()
                            .map(|v| v.join(",")),
                        value => scalar(value),
                    };
                    let value = value.ok_or_else(|| format!("`{name}` must not be an object"))?;
                    if value.contains(['\r', '\n']) {
                        return Err(format!("`{name}` must not contain a line break"));
                    }
                    headers.push((name.to_owned(), value));
                }
            }
        }

        let body = match endpoint.body {
            Body::None => None,
            Body::Flat { required } if args.is_empty() && !required => None,
            Body::Flat { .. } => Some(Value::Object(args)),
            Body::Field => args.remove("body").filter(|body| !body.is_null()),
        };
        let body = body.map(|body| {
            headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
            body.to_string()
        });
        Ok(HttpRequest {
            method: endpoint.method.clone(),
            url: format!("{}{path}{query}", base_url.trim_end_matches('/')),
            headers,
            body,
        })
    }
}

/// The response to an [`HttpRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HttpResponse {
    /// The status, such as 200.
    pub status: u16,
    /// The `Content-Type` header, if there is one.
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Returns the pairs that send `value` in the query as the parameter
/// `name`.
fn pairs(name: &str, value: Value, exploded: bool) -> Result<Vec<(String, String)>, String> {
    let scalar = |value: &Value| scalar(value).ok_or_else(|| format!("`{name}` must not nest arrays or objects"));
    Ok(match value {
        Value::Array(values) if exploded => values
            .iter()
            .map(|v| Ok((name.to_owned(), scalar(v)?)))
            .collect::<Result<_, String>>()?,
        Value::Array(values) => {
            let values = values.iter().map(scalar).collect::<Result<Vec<_>, _>>()?;
            vec![(name.to_owned(), values.join(","))]
        }
        Value::Object(entries) if exploded => entries
            .iter()
            .map(|(k, v)| Ok((k.clone(), scalar(v)?)))
            .collect::<Result<_, String>>()?,
        Value::Object(entries) => {
            let mut parts = Vec::new();
            for (k, v) in &entries {
                parts.push(k.clone());
                parts.push(scalar(v)?);
            }
            vec![(name.to_owned(), parts.join(","))]
        }
        value => vec![(name.to_owned(), scalar(&value)?)],
    })
}

/// Returns `value` as it is written in a path, query or header, or `None`
/// if it is null, an array or an object.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Returns `s` percent-encoded, leaving only unreserved characters as they
/// are, so it is safe in a path segment and in a query.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            write!(out, "%{byte:02X}").unwrap();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::HttpRequest;
    use crate::mcp::tools::{Body, Endpoint, Location, Param};

    /// Returns an endpoint that sends `params` and `body` with
    /// `POST /a/{id}`.
    fn endpoint(params: &[(&str, Location)], body: Body) -> Endpoint {
        Endpoint {
            method: "POST".to_owned(),
            path: "/a/{id}".to_owned(),
            params: params
                .iter()
                .map(|&(name, location)| Param {
                    name: name.to_owned(),
                    location,
                })
                .collect(),
            body,
        }
    }

    fn args(value: Value) -> Map<String, Value> {
        let Value::Object(args) = value else {
            panic!("not an object")
        };
        args
    }

    #[test]
    fn parameters() {
        let endpoint = endpoint(
            &[
                ("id", Location::Path),
                ("ids", Location::Query { exploded: true }),
                ("tags", Location::Query { exploded: false }),
                ("filter", Location::Query { exploded: true }),
                ("q", Location::Query { exploded: true }),
                ("X-Trace", Location::Header),
            ],
            Body::Flat { required: false },
        );
        let request = HttpRequest::new(
            &endpoint,
            "https://api.example.com/v1/",
            args(json!({
                "id": "a/b c", "ids": [1, 2], "tags": ["x", "y"], "filter": {"k": "v"},
                "q": null, "X-Trace": "t", "name": "n"
            })),
        )
        .unwrap();
        assert_eq!(
            request,
            HttpRequest {
                method: "POST".to_owned(),
                url: "https://api.example.com/v1/a/a%2Fb%20c?ids=1&ids=2&tags=x%2Cy&k=v".to_owned(),
                headers: vec![
                    ("X-Trace".to_owned(), "t".to_owned()),
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                ],
                body: Some(r#"{"name":"n"}"#.to_owned()),
            }
        );
    }

    #[test]
    fn missing_path_parameter() {
        let endpoint = endpoint(&[("id", Location::Path)], Body::None);
        let err = HttpRequest::new(&endpoint, "https://x", Map::new()).unwrap_err();
        assert_eq!(err, "missing argument `id`");
    }

    #[test]
    fn flat_bodies() {
        let path = [("id", Location::Path)];
        let optional = endpoint(&path, Body::Flat { required: false });
        let request = HttpRequest::new(&optional, "https://x", args(json!({"id": 1}))).unwrap();
        assert_eq!((request.url.as_str(), request.body), ("https://x/a/1", None));
        let required = endpoint(&path, Body::Flat { required: true });
        let request = HttpRequest::new(&required, "https://x", args(json!({"id": 1}))).unwrap();
        assert_eq!(request.body.as_deref(), Some("{}"));
    }

    #[test]
    fn field_body() {
        let endpoint = endpoint(&[("id", Location::Path)], Body::Field);
        let request =
            HttpRequest::new(&endpoint, "https://x", args(json!({"id": 1, "body": [1], "other": 2}))).unwrap();
        assert_eq!(request.body.as_deref(), Some("[1]"));
    }

    #[test]
    fn bad_header() {
        let endpoint = endpoint(&[("id", Location::Path), ("X-A", Location::Header)], Body::None);
        let err = HttpRequest::new(&endpoint, "https://x", args(json!({"id": 1, "X-A": "a\r\nB: b"}))).unwrap_err();
        assert!(err.contains("line break"), "{err}");
    }

    #[test]
    fn authorization() {
        let mut request = HttpRequest {
            method: "GET".to_owned(),
            url: "https://x".to_owned(),
            headers: Vec::new(),
            body: None,
        };
        request.bearer("t");
        request.bearer("u");
        assert_eq!(request.headers, [("Authorization".to_owned(), "Bearer t".to_owned())]);
    }
}
