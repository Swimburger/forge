//! The tools of a Forge document: one for each method.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use forge::{Err, Field, Forge, HttpMethod, Method, Model, Name, Type};
use rmcp::model::ToolAnnotations;
use serde_json::{Map, Value};

use super::schema::Schemas;

/// The tools of a document, and what the server tells a client about it.
pub(super) struct Api {
    /// A description of the API, which the server gives a client as its
    /// instructions. Empty when the document has none.
    pub description: String,
    /// The base URL of the API, from the first server in the document, or
    /// empty when it has none.
    pub base_url: String,
    /// Every tool, sorted by group and then by name.
    pub tools: Vec<Tool>,
    /// A line for each method of the selected groups that has no tool,
    /// naming it and saying why, such as `files.upload: its request body is
    /// not JSON`.
    pub skipped: Vec<String>,
}

/// A method of the API, exposed as a tool.
#[derive(Clone, Debug)]
pub(super) struct Tool {
    /// What clients see: the name of the tool, unique within the API, such
    /// as `accounts_members_list`, its title, description, the JSON Schema
    /// of its arguments, and hints of what calling it does.
    pub definition: rmcp::model::Tool,
    /// How calling the tool calls the API.
    pub endpoint: Endpoint,
}

/// The operation a [`Tool`] calls, and where its arguments are sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Endpoint {
    /// The HTTP method the operation is sent with, such as `GET`.
    pub method: String,
    /// The path template of the operation, such as `/accounts/{account_id}`.
    pub path: String,
    /// The arguments that are sent in the path, query or headers.
    pub params: Vec<Param>,
    /// Where the JSON request body comes from.
    pub body: Body,
}

/// An argument of a [`Tool`] that is sent in the path, query or headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Param {
    /// The name of the argument, and of the parameter on the wire.
    pub name: String,
    pub location: Location,
}

/// Where a [`Param`] is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Location {
    /// In the path, in place of `{name}`.
    Path,
    /// In the query. An exploded array is sent as a pair for each element,
    /// and an exploded object as a pair for each property; otherwise the
    /// value is sent as one pair, with its parts separated by commas.
    Query { exploded: bool },
    /// In the header named `name`, with an array's elements separated by
    /// commas.
    Header,
}

/// Where the JSON request body of a [`Tool`] comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Body {
    /// The operation has no JSON body.
    None,
    /// The body is the arguments that are not parameters. When there are
    /// none, `{}` is sent if the body is required, and nothing otherwise.
    Flat { required: bool },
    /// The body is the argument `body`.
    Field,
}

impl Api {
    /// Returns a tool for each method of `forge`, or only for those in
    /// `groups` and the groups inside them when `groups` is not empty.
    ///
    /// A group is written as the names of the groups from the top level
    /// down, separated by dots, such as `dns.records`, and names match when
    /// they are equal as [`Name`]s. A group that matches nothing is an
    /// error.
    ///
    /// A tool is named after its method's groups and its own name, in
    /// snake_case, joined by underscores, such as `dns_records_create`. Two
    /// methods with the same tool name are an error. Hidden methods have no
    /// tool, nor do methods whose request body is not JSON, or with two
    /// arguments of the same name: see [`Api::skipped`].
    pub fn new(forge: &Forge, groups: &[String]) -> Result<Api, Err> {
        let filters = groups
            .iter()
            .map(|group| {
                let path: Vec<String> = group.split('.').map(|name| Name::new(name).snake()).collect();
                if path.iter().any(String::is_empty) {
                    return Err(format!("{group:?} is not a group: write one as names separated by dots").into());
                }
                Ok(path)
            })
            .collect::<Result<Vec<_>, Err>>()?;
        let mut matched = vec![false; filters.len()];

        let mut tools = Vec::new();
        let mut skipped = Vec::new();
        let mut names: HashMap<String, String> = HashMap::new();
        for group in forge.groups() {
            let path: Vec<String> = group.path().map(Name::snake).collect();
            if !filters.is_empty() {
                let mut selected = false;
                for (filter, matched) in filters.iter().zip(&mut matched) {
                    if path.starts_with(filter) {
                        *matched = true;
                        selected = true;
                    }
                }
                if !selected {
                    continue;
                }
            }
            for m in group.methods() {
                let id = format!("{}.{}", path.join("."), m.name().snake());
                let tool = match tool(m, &path) {
                    Ok(tool) => tool,
                    Err(reason) => {
                        skipped.push(format!("{id}: {reason}"));
                        continue;
                    }
                };
                let name = tool.definition.name.to_string();
                if let Some(other) = names.insert(name.clone(), id.clone()) {
                    return Err(format!("{other} and {id} are both the tool {name}").into());
                }
                tools.push(tool);
            }
        }
        if let Some(i) = matched.iter().position(|&matched| !matched) {
            return Err(format!("there is no group {:?}", groups[i]).into());
        }

        Ok(Api {
            description: forge.description.trim().to_owned(),
            base_url: forge.base_url.clone(),
            tools,
            skipped,
        })
    }
}

/// Returns the tool of `m`, whose groups' names are `path`, or why it has
/// none.
fn tool(m: Method<'_>, path: &[String]) -> Result<Tool, String> {
    if m.is_hidden() {
        return Err("it is hidden".to_owned());
    }
    if let Some(body) = m.body()
        && body.json().is_none()
        && body.contents().len() > 0
    {
        return Err("its request body is not JSON".to_owned());
    }
    let request = Request::new(m);
    let mut names = HashSet::new();
    let body_name = matches!(request.body, Some(BodyKind::Field { .. })).then_some("body");
    let arguments = request
        .params
        .iter()
        .map(|(field, _)| field.wire_name())
        .chain(body_name);
    for name in arguments {
        if !names.insert(name) {
            return Err(format!("two of its arguments are named {name:?}"));
        }
    }

    let mut words = path.to_vec();
    words.push(m.name().snake());
    let title = m.summary().trim();
    let description = description(m);
    let mut annotations = annotations(m);
    let mut definition = rmcp::model::Tool::new_with_raw(
        words.join("_"),
        (!description.is_empty()).then(|| description.into()),
        Arc::new(request.schema()),
    );
    if !title.is_empty() {
        annotations.title = Some(title.to_owned());
        definition = definition.with_title(title);
    }
    Ok(Tool {
        definition: definition.with_annotations(annotations),
        endpoint: Endpoint {
            method: m.http_method().as_str().to_owned(),
            path: m.path().to_owned(),
            params: request
                .params
                .iter()
                .map(|&(field, location)| Param {
                    name: field.wire_name().to_owned(),
                    location,
                })
                .collect(),
            body: match request.body {
                None => Body::None,
                Some(BodyKind::Flat { required, .. }) => Body::Flat { required },
                Some(BodyKind::Field { .. }) => Body::Field,
            },
        },
    })
}

/// Returns the description of the tool of `m`: its summary and description,
/// then whether it is deprecated, and the sentence to confirm it with.
fn description(m: Method<'_>) -> String {
    let mut paragraphs: Vec<&str> = Vec::new();
    let summary = m.summary().trim();
    let description = m.description().trim();
    if !summary.is_empty() {
        paragraphs.push(summary);
    }
    if !description.is_empty() && description != summary {
        paragraphs.push(description);
    }
    if m.is_deprecated() {
        paragraphs.push("Deprecated.");
    }
    if let Some(confirmation) = m.confirmation().map(str::trim)
        && !confirmation.is_empty()
    {
        paragraphs.push(confirmation);
    }
    paragraphs.join("\n\n")
}

/// The arguments of a tool: the path, query and header parameters of its
/// method, each named after its wire name, and its JSON body.
struct Request<'f> {
    /// The parameters: path, then query, then headers.
    params: Vec<(Field<'f>, Location)>,
    body: Option<BodyKind<'f>>,
}

/// Where the body of a method is in the arguments of its tool.
enum BodyKind<'f> {
    /// The body's fields are arguments, next to the parameters.
    Flat { ty: Type<'f>, required: bool },
    /// The body is the argument `body`.
    Field {
        ty: Type<'f>,
        required: bool,
        description: &'f str,
    },
}

impl<'f> Request<'f> {
    /// Returns the arguments of the tool of `m`. The body is flat when
    /// [`Method::request`] puts its fields next to the parameters.
    fn new(m: Method<'f>) -> Request<'f> {
        let request = m.request();
        let object = match request {
            Some(Type::Object(object)) => Some(object),
            Some(Type::Intersection(intersection)) => match intersection.members().next() {
                Some(Type::Object(object)) => Some(object),
                _ => None,
            },
            _ => None,
        };
        let mut params: Vec<(Field<'f>, Location)> = object
            .into_iter()
            .flat_map(|object| object.fields())
            .filter_map(|field| {
                let location = match m.location(field)? {
                    forge::Location::Path => Location::Path,
                    forge::Location::Query { exploded } => Location::Query { exploded },
                };
                Some((field, location))
            })
            .collect();
        let has_params = !params.is_empty();
        params.extend(
            m.headers()
                .into_iter()
                .flat_map(|headers| headers.fields())
                .map(|field| (field, Location::Header)),
        );
        let body = m.body().and_then(|body| {
            let ty = body.json()?.ty();
            let required = body.is_required();
            let flat = if has_params {
                matches!(request, Some(Type::Intersection(_)))
            } else {
                object_like(ty, &mut HashSet::new())
            };
            Some(if flat {
                BodyKind::Flat { ty, required }
            } else {
                BodyKind::Field {
                    ty,
                    required,
                    description: body.description(),
                }
            })
        });
        Request { params, body }
    }

    /// Returns the JSON Schema of the arguments.
    fn schema(&self) -> Map<String, Value> {
        let mut schemas = Schemas::default();
        let mut properties = Map::new();
        let mut required = Vec::new();
        for (field, _) in &self.params {
            properties.insert(field.wire_name().to_owned(), schemas.field(*field));
            if field.is_required() {
                required.push(field.wire_name().to_owned());
            }
        }
        let mut all_of = Vec::new();
        match self.body {
            None => {}
            Some(BodyKind::Flat { ty, .. }) => all_of.push(schemas.ty(ty)),
            Some(BodyKind::Field {
                ty,
                required: body_required,
                description,
            }) => {
                let mut schema = schemas.ty(ty);
                if let Value::Object(map) = &mut schema
                    && !description.is_empty()
                {
                    map.insert("description".to_owned(), description.into());
                }
                properties.insert("body".to_owned(), schema);
                if body_required {
                    required.push("body".to_owned());
                }
            }
        }
        schemas.object(properties, required, all_of)
    }
}

/// Reports whether `t` is an object with fields, or a model, union or
/// intersection of them, whose fields can sit next to the parameters.
fn object_like<'f>(t: Type<'f>, seen: &mut HashSet<Model<'f>>) -> bool {
    match t {
        Type::Object(object) => object.fields().len() > 0,
        Type::Model(m) => seen.insert(m) && object_like(m.ty(), seen),
        Type::Union(union) => union.members().all(|m| object_like(m, seen)),
        Type::Intersection(intersection) => intersection.members().all(|m| object_like(m, seen)),
        _ => false,
    }
}

/// Returns the hints of what calling `m` does, from its HTTP method. A
/// method that needs confirmation is destructive. Every tool calls an API
/// outside the server, so each is open-world.
fn annotations(m: Method<'_>) -> ToolAnnotations {
    let confirm = m.confirmation().is_some();
    let (read_only, destructive, idempotent) = match m.http_method() {
        HttpMethod::Get | HttpMethod::Head | HttpMethod::Options | HttpMethod::Query => (!confirm, confirm, true),
        HttpMethod::Put | HttpMethod::Delete => (false, true, true),
        HttpMethod::Post | HttpMethod::Patch | HttpMethod::Trace | HttpMethod::Connect | HttpMethod::Other(_) => {
            (false, confirm, false)
        }
    };
    ToolAnnotations::new()
        .read_only(read_only)
        .destructive(destructive)
        .idempotent(idempotent)
        .open_world(true)
}

#[cfg(test)]
mod tests {
    use forge::{Err, Forge};
    use serde_json::{Value, json};

    use super::{Api, Body, Endpoint, Location, Param, Tool};

    /// Returns the tools of `spec`, in `groups`, or why it has none.
    fn try_build(spec: &str, groups: &[&str]) -> Result<Api, Err> {
        let forge: Forge = serde_json::from_str(spec)?;
        let groups: Vec<String> = groups.iter().map(|&g| g.to_owned()).collect();
        Api::new(&forge, &groups)
    }

    /// Returns the tools of `spec`, in `groups`.
    fn build(spec: &str, groups: &[&str]) -> Api {
        try_build(spec, groups).unwrap()
    }

    /// Returns the only tool of `spec`.
    fn only(spec: &str) -> Tool {
        let api = build(spec, &[]);
        assert_eq!(api.tools.len(), 1, "{:?}", api.skipped);
        api.tools.into_iter().next().unwrap()
    }

    /// Returns the definition of `tool`, as clients see it.
    fn definition(tool: &Tool) -> Value {
        serde_json::to_value(&tool.definition).unwrap()
    }

    /// Returns the names of the tools of `api`.
    fn names(api: &Api) -> Vec<&str> {
        api.tools.iter().map(|t| t.definition.name.as_ref()).collect()
    }

    #[test]
    fn document() {
        let api = build(
            r#"{"openapi": "3.1.0", "info": {"title": "Example", "version": "1.2"},
                "servers": [{"url": "https://api.example.com/v1"}], "paths": {}}"#,
            &[],
        );
        assert_eq!(api.description, "Example");
        assert_eq!(api.base_url, "https://api.example.com/v1");
        assert!(api.tools.is_empty());
        assert!(api.skipped.is_empty());
    }

    #[test]
    fn get_with_parameters() {
        let tool = only(
            r#"{"openapi": "3.1.0", "info": {"title": "t", "version": "1"},
                "paths": {"/accounts/{account_id}/members": {"get": {
                    "x-fern-sdk-group-name": "accounts.members", "x-fern-sdk-method-name": "list",
                    "summary": "List members",
                    "parameters": [
                        {"name": "account_id", "in": "path", "required": true, "schema": {"type": "string"},
                         "description": "The account."},
                        {"name": "page", "in": "query", "schema": {"type": "integer", "default": 1}},
                        {"name": "status", "in": "query", "explode": false,
                         "schema": {"type": "array", "items": {"enum": ["active", "pending"]}}},
                        {"name": "X-Request-Id", "in": "header", "schema": {"type": "string", "format": "uuid"}}
                    ],
                    "responses": {"200": {"description": "ok"}}}}}}"#,
        );
        let param = |name: &str, location| Param {
            name: name.to_owned(),
            location,
        };
        assert_eq!(
            tool.endpoint,
            Endpoint {
                method: "GET".to_owned(),
                path: "/accounts/{account_id}/members".to_owned(),
                params: vec![
                    param("account_id", Location::Path),
                    param("page", Location::Query { exploded: true }),
                    param("status", Location::Query { exploded: false }),
                    param("X-Request-Id", Location::Header),
                ],
                body: Body::None,
            }
        );
        assert_eq!(
            definition(&tool),
            json!({
                "name": "accounts_members_list",
                "title": "List members",
                "description": "List members",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "account_id": {"type": "string", "description": "The account."},
                        "page": {"type": "integer", "default": 1},
                        "status": {"type": "array", "items": {"enum": ["active", "pending"]}},
                        "X-Request-Id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["account_id"]
                },
                "annotations": {
                    "title": "List members",
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": true
                }
            })
        );
    }

    #[test]
    fn flat_body_with_model() {
        let tool = only(
            r##"{"openapi": "3.1.0", "info": {"title": "t", "version": "1"},
                "paths": {"/zones/{zone_id}/records": {"post": {
                    "x-fern-sdk-group-name": "dns.records", "x-fern-sdk-method-name": "create",
                    "parameters": [{"name": "zone_id", "in": "path", "required": true, "schema": {"type": "string"}}],
                    "requestBody": {"required": true, "content": {"application/json": {
                        "schema": {"$ref": "#/components/schemas/Record"}}}},
                    "responses": {"200": {"description": "ok"}}}}},
                "components": {"schemas": {"Record": {"type": "object", "required": ["name"],
                    "properties": {"id": {"type": "string", "readOnly": true}, "name": {"type": "string"},
                                   "children": {"type": "array", "items": {"$ref": "#/components/schemas/Record"}}}}}}}"##,
        );
        assert_eq!(tool.endpoint.body, Body::Flat { required: true });
        let definition = definition(&tool);
        assert_eq!(
            definition["annotations"],
            json!({"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true})
        );
        assert_eq!(
            definition["inputSchema"],
            json!({
                "type": "object",
                "properties": {"zone_id": {"type": "string"}},
                "required": ["zone_id"],
                "allOf": [{"$ref": "#/$defs/Record"}],
                "$defs": {"Record": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "children": {"type": "array", "items": {"$ref": "#/$defs/Record"}}
                    },
                    "required": ["name"]
                }}
            })
        );
    }

    #[test]
    fn array_body_is_a_field() {
        let tool = only(
            r#"{"openapi": "3.1.0", "info": {"title": "t", "version": "1"},
                "paths": {"/keys": {"delete": {
                    "x-fern-sdk-group-name": "kv", "x-fern-sdk-method-name": "bulk_delete",
                    "x-forge-require-confirmation": "This operation deletes the keys.",
                    "requestBody": {"required": true, "description": "The keys.", "content": {"application/json": {
                        "schema": {"type": "array", "items": {"type": "string"}}}}},
                    "responses": {"200": {"description": "ok"}}}}}}"#,
        );
        assert_eq!(tool.endpoint.body, Body::Field);
        let definition = definition(&tool);
        assert_eq!(definition["name"], "kv_bulk_delete");
        assert_eq!(definition["description"], "This operation deletes the keys.");
        assert_eq!(definition["annotations"]["destructiveHint"], true);
        assert_eq!(
            definition["inputSchema"],
            json!({
                "type": "object",
                "properties": {"body": {"type": "array", "items": {"type": "string"}, "description": "The keys."}},
                "required": ["body"]
            })
        );
    }

    #[test]
    fn skips() {
        let built = build(
            r#"{"openapi": "3.1.0", "info": {"title": "t", "version": "1"},
                "paths": {
                    "/hidden": {"get": {"x-fern-sdk-group-name": "a", "x-fern-sdk-method-name": "hidden",
                        "x-forge-hidden": true, "responses": {"200": {"description": "ok"}}}},
                    "/upload": {"put": {"x-fern-sdk-group-name": "a", "x-fern-sdk-method-name": "upload",
                        "requestBody": {"content": {"application/octet-stream": {"schema": {"type": "string"}}}},
                        "responses": {"200": {"description": "ok"}}}},
                    "/clash": {"get": {"x-fern-sdk-group-name": "a", "x-fern-sdk-method-name": "clash",
                        "parameters": [{"name": "key", "in": "query", "schema": {"type": "string"}},
                                       {"name": "key", "in": "header", "schema": {"type": "string"}}],
                        "responses": {"200": {"description": "ok"}}}},
                    "/text": {"get": {"x-fern-sdk-group-name": "a", "x-fern-sdk-method-name": "text",
                        "responses": {"200": {"description": "ok",
                            "content": {"text/csv": {"schema": {"type": "string"}}}}}}}
                }}"#,
            &[],
        );
        assert_eq!(names(&built), ["a_text"]);
        assert_eq!(
            built.skipped,
            [
                r#"a.clash: two of its arguments are named "key""#,
                "a.hidden: it is hidden",
                "a.upload: its request body is not JSON",
            ]
        );
    }

    /// A document with methods in `a`, `a.b` and `c`.
    const GROUPS: &str = r#"{"openapi": "3.1.0", "info": {"title": "t", "version": "1"},
        "paths": {
            "/a": {"get": {"x-fern-sdk-group-name": "a", "x-fern-sdk-method-name": "get",
                "responses": {"200": {"description": "ok"}}}},
            "/a/b": {"get": {"x-fern-sdk-group-name": "a.b", "x-fern-sdk-method-name": "get",
                "responses": {"200": {"description": "ok"}}}},
            "/c": {"get": {"x-fern-sdk-group-name": "c", "x-fern-sdk-method-name": "get",
                "responses": {"200": {"description": "ok"}}}}
        }}"#;

    #[test]
    fn groups() {
        assert_eq!(names(&build(GROUPS, &[])), ["a_get", "a_b_get", "c_get"]);
        assert_eq!(names(&build(GROUPS, &["a"])), ["a_get", "a_b_get"]);
        assert_eq!(names(&build(GROUPS, &["A.B"])), ["a_b_get"]);
        assert_eq!(names(&build(GROUPS, &["a.b", "c"])), ["a_b_get", "c_get"]);
    }

    #[test]
    fn unknown_group() {
        let err = try_build(GROUPS, &["a.x"]).err().unwrap();
        assert_eq!(err.to_string(), r#"there is no group "a.x""#);
        let err = try_build(GROUPS, &["a..b"]).err().unwrap();
        assert!(err.to_string().contains("is not a group"), "{err}");
    }

    #[test]
    fn duplicate_names() {
        let err = try_build(
            r#"{"openapi": "3.1.0", "info": {"title": "t", "version": "1"},
                "paths": {
                    "/a": {"get": {"x-fern-sdk-group-name": "a.b", "x-fern-sdk-method-name": "c",
                        "responses": {"200": {"description": "ok"}}}},
                    "/b": {"get": {"x-fern-sdk-group-name": "a", "x-fern-sdk-method-name": "b_c",
                        "responses": {"200": {"description": "ok"}}}}
                }}"#,
            &[],
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("are both the tool a_b_c"), "{err}");
    }
}
