//! JSON Schemas of the arguments of tools.
//!
//! A tool's input schema is an object with a property for each argument.
//! Models are in `$defs` and referenced with `$ref`, so recursive models
//! terminate. Read-only fields are left out, since the schema only
//! describes what a client sends.

use std::collections::BTreeMap;

use forge::{EnumValue, Field, Json, Model, Object, Scalar, Type};
use serde_json::{Map, Value, json};

/// Builds the schemas of the arguments of one tool, collecting the models
/// they use.
#[derive(Default)]
pub(super) struct Schemas<'f> {
    /// The models referenced so far, by schema name, with their schemas.
    /// A model is in the map before its schema is built, so a model that
    /// refers to itself is built once.
    defs: BTreeMap<&'f str, Option<Value>>,
}

impl<'f> Schemas<'f> {
    /// Returns the schema of an object with `properties`, which must be
    /// present when listed in `required`, and that is also each schema in
    /// `all_of`. The models every schema so far uses are in its `$defs`.
    pub(super) fn object(self, properties: Map<String, Value>, required: Vec<String>, all_of: Vec<Value>) -> Value {
        let mut schema = Map::new();
        schema.insert("type".to_owned(), json!("object"));
        schema.insert("properties".to_owned(), Value::Object(properties));
        if !required.is_empty() {
            schema.insert("required".to_owned(), json!(required));
        }
        if !all_of.is_empty() {
            schema.insert("allOf".to_owned(), Value::Array(all_of));
        }
        if !self.defs.is_empty() {
            let defs = self
                .defs
                .into_iter()
                .map(|(name, schema)| (name.to_owned(), schema.unwrap_or_else(|| json!({}))))
                .collect();
            schema.insert("$defs".to_owned(), Value::Object(defs));
        }
        Value::Object(schema)
    }

    /// Returns the schema of a property for `field`: the schema of its
    /// type, with its description and default.
    pub(super) fn field(&mut self, field: Field<'f>) -> Value {
        let mut schema = self.ty(field.ty());
        if let Value::Object(map) = &mut schema {
            if !field.description().is_empty() {
                map.insert("description".to_owned(), json!(field.description()));
            }
            if let Some(default) = field.default() {
                map.insert("default".to_owned(), value(default));
            }
            if field.is_deprecated() {
                map.insert("deprecated".to_owned(), json!(true));
            }
        }
        schema
    }

    /// Returns the schema of `t`.
    pub(super) fn ty(&mut self, t: Type<'f>) -> Value {
        match t {
            Type::Scalar(scalar) => scalar_schema(scalar),
            Type::Enum(e) => {
                let values: Vec<Value> = e
                    .values()
                    .map(|v| match v {
                        EnumValue::String(s) => json!(s),
                        EnumValue::Number(n) => number(n),
                        EnumValue::Boolean(b) => json!(b),
                    })
                    .collect();
                json!({ "enum": values })
            }
            Type::Array(array) => {
                let mut schema = Map::new();
                schema.insert("type".to_owned(), json!("array"));
                schema.insert("items".to_owned(), self.ty(array.elem()));
                if let Some(max) = array.max_items() {
                    schema.insert("maxItems".to_owned(), json!(max));
                }
                Value::Object(schema)
            }
            Type::Object(object) => self.object_schema(object),
            Type::Union(union) => {
                let members: Vec<Value> = union.members().map(|m| self.ty(m)).collect();
                json!({ "anyOf": members })
            }
            Type::Intersection(intersection) => {
                let members: Vec<Value> = intersection.members().map(|m| self.ty(m)).collect();
                json!({ "allOf": members })
            }
            Type::Model(model) => self.model(model),
        }
    }

    /// Returns a reference to the schema of `model`, building it the first
    /// time.
    fn model(&mut self, model: Model<'f>) -> Value {
        let name = model.schema_name();
        if !self.defs.contains_key(name) {
            self.defs.insert(name, None);
            let mut schema = self.ty(model.ty());
            if let Value::Object(map) = &mut schema
                && !model.description().is_empty()
                && !map.contains_key("description")
            {
                map.insert("description".to_owned(), json!(model.description()));
            }
            self.defs.insert(name, Some(schema));
        }
        json!({ "$ref": format!("#/$defs/{}", pointer(name)) })
    }

    /// Returns the schema of `object`. It allows keys other than its
    /// fields, since a flat body's schema is one member of an `allOf` with
    /// the parameters, which the body's object would otherwise reject.
    fn object_schema(&mut self, object: Object<'f>) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for field in object.fields().filter(|f| !f.is_read_only()) {
            properties.insert(field.wire_name().to_owned(), self.field(field));
            if field.is_required() {
                required.push(field.wire_name());
            }
        }
        let mut schema = Map::new();
        schema.insert("type".to_owned(), json!("object"));
        if properties.is_empty() {
            if let Some(extra) = object.extra()
                && !matches!(extra, Type::Scalar(Scalar::Unknown))
            {
                schema.insert("additionalProperties".to_owned(), self.ty(extra));
            }
        } else {
            schema.insert("properties".to_owned(), Value::Object(properties));
        }
        if !required.is_empty() {
            schema.insert("required".to_owned(), json!(required));
        }
        Value::Object(schema)
    }
}

/// Returns the schema of `scalar`.
fn scalar_schema(scalar: Scalar) -> Value {
    match scalar {
        Scalar::Unknown => json!({}),
        Scalar::Null => json!({ "type": "null" }),
        Scalar::Boolean => json!({ "type": "boolean" }),
        Scalar::String | Scalar::Binary => json!({ "type": "string" }),
        Scalar::Number(n) if n.is_integer() => json!({ "type": "integer" }),
        Scalar::Number(_) => json!({ "type": "number" }),
        Scalar::DateTime => json!({ "type": "string", "format": "date-time" }),
        Scalar::Date => json!({ "type": "string", "format": "date" }),
        Scalar::Bytes => json!({ "type": "string", "contentEncoding": "base64" }),
    }
}

/// Returns `json` as a JSON value.
fn value(json: &Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(b) => json!(b),
        Json::Number(n) => number(*n),
        Json::String(s) => json!(s),
        Json::Array(values) => Value::Array(values.iter().map(value).collect()),
        Json::Object(entries) => Value::Object(entries.iter().map(|(k, v)| (k.clone(), value(v))).collect()),
    }
}

/// Returns `n` as a JSON number, written without a fraction when it is an
/// integer, as `1` rather than `1.0`.
fn number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 2f64.powi(53) {
        return json!(n as i64);
    }
    serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
}

/// Returns `name` escaped as a token of a JSON Pointer.
fn pointer(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::{number, pointer};
    use serde_json::json;

    #[test]
    fn numbers() {
        assert_eq!(number(1.0), json!(1));
        assert_eq!(number(-3.0), json!(-3));
        assert_eq!(number(1.5), json!(1.5));
    }

    #[test]
    fn pointers() {
        assert_eq!(pointer("Tree/$defs/Node"), "Tree~1$defs~1Node");
        assert_eq!(pointer("a~b"), "a~0b");
    }
}
