//! Helpers to build documents and to compare what two of them make, so that
//! each test is about one thing.

use forge::normalize::{Options, normalize};
use forge::{Forge, Type};

/// Parses `json` as a whole document, as `forge` reads it: only the native
/// keys mean anything.
#[track_caller]
pub fn parse(json: &str) -> Forge {
    match serde_json::from_str(json) {
        Ok(forge) => forge,
        Err(err) => panic!("parse: {err}"),
    }
}

/// Parses `json` as a whole document after `forge normalize` has rewritten the
/// keys of other tools in it, as the commands are run one after the other.
#[track_caller]
pub fn parse_normalized(json: &str) -> Forge {
    parse(&normalized(json))
}

/// Returns `json` after `forge normalize`.
#[track_caller]
pub fn normalized(json: &str) -> String {
    let normalized = match normalize(json.as_bytes(), &Options::default()) {
        Ok(normalized) => normalized,
        Err(err) => panic!("normalize: {err}"),
    };
    normalized
        .json
        .map_or_else(|| json.to_owned(), |edited| String::from_utf8(edited.to_vec()).unwrap())
}

/// Runs `forge normalize` on `json`, and returns what it made of it, and the
/// keys it left as they are written, each as where, the key and why.
#[track_caller]
pub fn normalized_and_left(json: &str) -> (String, Vec<(String, String, &'static str)>) {
    let normalized = match normalize(json.as_bytes(), &Options::default()) {
        Ok(normalized) => normalized,
        Err(err) => panic!("normalize: {err}"),
    };
    let left = normalized
        .report
        .left
        .iter()
        .map(|left| (left.location.clone(), left.key.clone(), left.why))
        .collect();
    let text = normalized
        .json
        .map_or_else(|| json.to_owned(), |edited| String::from_utf8(edited.to_vec()).unwrap());
    (text, left)
}

/// Parses `json`, which must fail, and returns the error message.
#[track_caller]
pub fn parse_err(json: &str) -> String {
    match serde_json::from_str::<Forge>(json) {
        Ok(_) => panic!("parse: want an error, got a Forge"),
        Err(err) => err.to_string(),
    }
}

/// Returns an OpenAPI 3.0.3 document with one operation, `GET /a`, that has
/// the members `keys`, written as the inside of a JSON object, each followed
/// by a comma, such as `"x-forge-method-name": "get",`.
pub fn operation_with(keys: &str) -> String {
    format!(
        r#"{{"openapi": "3.0.3", "info": {{"title": "t", "version": "1"}},
          "paths": {{"/a": {{"get": {{{keys} "responses": {{"204": {{"description": "ok"}}}}}}}}}}}}"#
    )
}

/// Returns an OpenAPI 3.0.3 document with one method, `a.get`, whose JSON
/// response has the schema `schema`, and `components.schemas` of `schemas`.
pub fn responding_with(schema: &str, schemas: &str) -> String {
    format!(
        r#"{{"openapi": "3.0.3", "info": {{"title": "t", "version": "1"}},
          "paths": {{"/a": {{"get": {{"x-forge-group-name": ["a"], "x-forge-method-name": "get",
            "responses": {{"200": {{"description": "ok", "content": {{"application/json": {{"schema": {schema}}}}}}}}}}}}}}},
          "components": {{"schemas": {schemas}}}}}"#
    )
}

/// Asserts that `a` and `b` build the same `Forge` after `forge normalize`, as
/// far as `describe` says.
#[track_caller]
pub fn assert_same(a: &str, b: &str) {
    let (a, b) = (describe(&parse_normalized(a)), describe(&parse_normalized(b)));
    assert_eq!(a, b, "the documents build different Forges");
}

/// Writes what a `Forge` says about every group, method and model, down to
/// the name of each field, so that two `Forge`s that differ in any name,
/// availability or shape differ in what this returns.
pub fn describe(forge: &Forge) -> String {
    let mut out = String::new();
    for group in forge.groups() {
        let path: Vec<String> = group.path().map(|name| name.snake()).collect();
        out += &format!("group {}\n", path.join("."));
        for method in group.methods() {
            let request = method.request().map_or_else(|| "-".to_string(), shape);
            let response = method
                .responses()
                .map(|response| response.json().map_or_else(|| "-".to_string(), |json| shape(json.ty())))
                .collect::<Vec<_>>()
                .join(",");
            out += &format!(
                "  method {} {} {} availability={:?} hidden={} request={request} response={response}\n",
                method.name().snake(),
                method.http_method().as_str(),
                method.path(),
                method.availability().map(|availability| availability.as_str()),
                method.is_hidden(),
            );
        }
        for model in group.models() {
            out += &format!("  model {} = {}\n", model.name().snake(), shape(model.ty()));
        }
    }
    for model in forge.models() {
        out += &format!("model {} = {}\n", model.name().snake(), shape(model.ty()));
    }
    out
}

/// Writes a type, with the wire name and name of every field, and a model by
/// name.
fn shape(ty: Type<'_>) -> String {
    match ty {
        Type::Scalar(scalar) => format!("{scalar:?}"),
        Type::Enum(values) => format!("enum{values:?}"),
        Type::Array(array) => format!("[{}]", shape(array.elem())),
        Type::Object(object) => {
            let fields: Vec<String> = object
                .fields()
                .map(|field| {
                    let secret = if field.is_sensitive() { "!" } else { "" };
                    format!(
                        "{}:{}={}{secret}",
                        field.wire_name(),
                        field.name().snake(),
                        shape(field.ty())
                    )
                })
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Type::Union(union) => union.members().map(shape).collect::<Vec<_>>().join("|"),
        Type::Intersection(intersection) => intersection.members().map(shape).collect::<Vec<_>>().join("&"),
        Type::Model(model) => format!("Model({})", model.name().snake()),
    }
}
