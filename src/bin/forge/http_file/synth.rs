//! Deterministic value synthesis from Forge types for `.http` files.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;

use forge::{Array, EnumValue, Field, Json, Model, Name, Object, Scalar, Type};
use serde_json::{Map, Number, Value};

/// Maximum nesting depth when synthesizing request bodies.
pub(super) const MAX_DEPTH: usize = 5;

/// Maximum number of JSON values synthesized in a single request body.
pub(super) const MAX_NODES: usize = 250;

/// Per-body synthesis state: nesting depth, container budget, cycle guard,
/// declared non-secret variable names to avoid shadowing, and any undeclared
/// secret variable names referenced.
#[derive(Clone, Default)]
pub(super) struct SynthCtx<'f> {
    depth: usize,
    nodes: usize,
    active_models: HashSet<Model<'f>>,
    declared_vars: HashSet<String>,
    pub(super) secrets: BTreeSet<String>,
}

impl<'f> SynthCtx<'f> {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn with_declared(declared_vars: HashSet<String>) -> Self {
        Self {
            declared_vars,
            ..Self::default()
        }
    }

    fn consume_node(&mut self) -> bool {
        if self.nodes >= MAX_NODES {
            return false;
        }
        self.nodes += 1;
        true
    }

    pub(super) fn record_secret(&mut self, var: &str) {
        self.secrets.insert(var.to_owned());
    }

    /// Allocates a secret variable name that does not collide with any
    /// declared non-sensitive `@var` in the same file.
    pub(super) fn secret_var(&mut self, name: &Name, wire_name: &str) -> String {
        let base = sanitize_var_name(name, wire_name);
        let var = if !self.declared_vars.contains(&base) {
            base
        } else {
            let mut chosen = format!("{base}_secret");
            if self.declared_vars.contains(&chosen) {
                for n in 2u32.. {
                    let candidate = format!("{base}_secret_{n}");
                    if !self.declared_vars.contains(&candidate) {
                        chosen = candidate;
                        break;
                    }
                }
            }
            chosen
        };
        self.record_secret(&var);
        var
    }
}

/// Allocates unique, `.http`-safe variable names within one file.
#[derive(Default)]
pub(super) struct VarScope {
    used: HashSet<String>,
}

impl VarScope {
    /// Reserves `name` so subsequent allocations avoid shadowing it.
    pub(super) fn reserve(&mut self, name: &str) {
        self.used.insert(name.to_owned());
    }

    /// Allocates a unique ASCII `snake_case` variable name for `name` (falling
    /// back to `wire_name`), disambiguating collisions with `_2`, `_3`, ...
    pub(super) fn alloc(&mut self, name: &Name, wire_name: &str) -> String {
        let base = sanitize_var_name(name, wire_name);
        if self.used.insert(base.clone()) {
            return base;
        }
        for n in 2u32.. {
            let candidate = format!("{base}_{n}");
            if self.used.insert(candidate.clone()) {
                return candidate;
            }
        }
        unreachable!("u32 exhausted")
    }
}

/// Sanitizes `wire_name` into a safe single path segment filename stem
/// (`[a-zA-Z0-9_-]`), preventing `/` or `..` traversal in `< ./<file>.bin`.
pub(super) fn sanitize_filename_stem(wire_name: &str) -> String {
    let mut out = String::with_capacity(wire_name.len());
    for ch in wire_name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() { "file".to_owned() } else { out }
}

/// Escapes `s` for use inside a quoted-string parameter in `Content-Disposition`.
pub(super) fn escape_quoted_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\r' | '\n' => out.push(' '),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out
}

/// Strips `\r` bytes so generated `.http` files always use LF line endings.
pub(super) fn normalize_lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Returns an ASCII `[a-z0-9_]+` variable name, prefixed with `_` if it starts
/// with a digit.
pub(super) fn sanitize_var_name(name: &Name, wire_name: &str) -> String {
    let raw = if name.is_empty() {
        wire_name.to_ascii_lowercase()
    } else {
        name.snake()
    };
    let mut out = String::with_capacity(raw.len());
    let mut prev_underscore = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !out.is_empty() && !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        return "var".to_owned();
    }
    if out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

/// A synthesized scalar value: either a plain literal or an undeclared
/// `{{var}}` template reference that must not be percent-encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ScalarVal {
    Literal(String),
    TemplateVar(String),
}

impl ScalarVal {
    /// Formats the value for contexts that do not percent-encode (such as
    /// HTTP headers, multipart text parts, or raw text bodies).
    pub(super) fn as_plain(&self) -> String {
        match self {
            ScalarVal::Literal(s) => s.clone(),
            ScalarVal::TemplateVar(var) => format!("{{{{{var}}}}}"),
        }
    }

    /// Formats the value for URL query strings or `application/x-www-form-urlencoded`,
    /// percent-encoding literals while keeping `{{var}}` intact.
    pub(super) fn as_encoded(&self) -> String {
        match self {
            ScalarVal::Literal(s) => percent_encode(s),
            ScalarVal::TemplateVar(var) => format!("{{{{{var}}}}}"),
        }
    }
}

/// Percent-encodes `s` per RFC 3986, leaving unreserved bytes unchanged.
pub(super) fn percent_encode(s: &str) -> String {
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

/// Returns how many elements to synthesize for an array schema.
pub(super) fn array_count(arr: Array<'_>) -> usize {
    if arr.max_items() == Some(0) {
        return 0;
    }
    if arr.min_items().is_some_and(|n| n >= 2) {
        return 2;
    }
    1
}

/// Unwraps `Model` and nullable `Union` wrappers (breaking cycles) to inspect
/// the underlying structural type.
pub(super) fn unwrap_type<'f>(mut ty: Type<'f>) -> Type<'f> {
    let mut seen = HashSet::new();
    loop {
        match ty {
            Type::Model(m) => {
                if !seen.insert(m) {
                    return Type::Scalar(Scalar::Unknown);
                }
                ty = m.ty();
            }
            Type::Union(u) => {
                let non_null: Vec<Type<'f>> = u
                    .members()
                    .filter(|m| !matches!(m, Type::Scalar(Scalar::Null)))
                    .collect();
                if non_null.len() == 1 {
                    ty = non_null[0];
                } else {
                    return ty;
                }
            }
            other => return other,
        }
    }
}

/// Reports whether `ty` (after unwrapping `Model` and nullable `Union`) is
/// `Scalar::Binary`.
pub(super) fn is_binary(ty: Type<'_>) -> bool {
    matches!(unwrap_type(ty), Type::Scalar(Scalar::Binary))
}

/// Reports whether `ty` (after unwrapping `Model` and nullable `Union`) is an
/// array whose elements are `Scalar::Binary`, returning its synthesized count.
pub(super) fn binary_array_count(ty: Type<'_>) -> Option<usize> {
    let Type::Array(arr) = unwrap_type(ty) else {
        return None;
    };
    is_binary(arr.elem()).then(|| array_count(arr))
}

/// Collects the effective non-`read_only` fields of an object or intersection
/// of objects, sorted by `wire_name`. Returns `None` if `ty` does not resolve
/// to an object shape.
pub(super) fn object_fields<'f>(ty: Type<'f>) -> Option<(Vec<Field<'f>>, Option<Type<'f>>)> {
    let mut seen = HashSet::new();
    let mut fields: BTreeMap<&'f str, Field<'f>> = BTreeMap::new();
    let mut extra: Option<Type<'f>> = None;
    if !collect_object_fields(ty, &mut seen, &mut fields, &mut extra) {
        return None;
    }
    Some((fields.into_values().collect(), extra))
}

fn collect_object_fields<'f>(
    ty: Type<'f>,
    seen: &mut HashSet<Model<'f>>,
    fields: &mut BTreeMap<&'f str, Field<'f>>,
    extra: &mut Option<Type<'f>>,
) -> bool {
    match ty {
        Type::Model(m) => {
            if !seen.insert(m) {
                return false;
            }
            let ok = collect_object_fields(m.ty(), seen, fields, extra);
            seen.remove(&m);
            ok
        }
        Type::Object(obj) => {
            for field in obj.fields() {
                if !field.is_read_only() {
                    fields
                        .entry(field.wire_name())
                        .and_modify(|existing| {
                            // Prefer a field with a default (or sensitive flag) deterministically
                            // when an intersection's unordered members declare the same wire name.
                            if (existing.default().is_none() && field.default().is_some())
                                || (!existing.is_sensitive() && field.is_sensitive())
                            {
                                *existing = field;
                            }
                        })
                        .or_insert(field);
                }
            }
            if extra.is_none() {
                *extra = obj.extra();
            }
            true
        }
        Type::Intersection(inter) => {
            let mut any = false;
            for member in inter.members() {
                if collect_object_fields(member, seen, fields, extra) {
                    any = true;
                }
            }
            any
        }
        Type::Union(u) => {
            let non_null: Vec<Type<'f>> = u
                .members()
                .filter(|m| !matches!(m, Type::Scalar(Scalar::Null)))
                .collect();
            if non_null.len() == 1 {
                collect_object_fields(non_null[0], seen, fields, extra)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Synthesizes a deterministic JSON value for `ty` (optionally from `field`).
pub(super) fn synth_json<'f>(ty: Type<'f>, field: Option<Field<'f>>, ctx: &mut SynthCtx<'f>) -> Value {
    if let Some(f) = field {
        if f.is_sensitive() {
            let var = ctx.secret_var(f.name(), f.wire_name());
            return Value::String(format!("{{{{{var}}}}}"));
        }
        if let Some(default) = f.default() {
            return forge_json_to_value(default);
        }
    }

    match ty {
        Type::Scalar(s) => synth_scalar_json(s),
        Type::Enum(e) => match e.values().next() {
            Some(EnumValue::String(s)) => Value::String(s.to_owned()),
            Some(EnumValue::Number(n)) => Value::Number(json_number(n)),
            Some(EnumValue::Boolean(b)) => Value::Bool(b),
            None => Value::Null,
        },
        Type::Array(arr) => {
            if ctx.depth >= MAX_DEPTH || !ctx.consume_node() {
                return Value::Array(Vec::new());
            }
            let count = array_count(arr);
            if count == 0 || is_active_model(arr.elem(), &ctx.active_models) {
                return Value::Array(Vec::new());
            }
            ctx.depth += 1;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(synth_json(arr.elem(), None, ctx));
            }
            ctx.depth -= 1;
            Value::Array(items)
        }
        Type::Object(obj) => synth_object_json(obj, ctx),
        Type::Model(m) => {
            if !ctx.active_models.insert(m) {
                return Value::Object(Map::new());
            }
            let value = synth_json(m.ty(), None, ctx);
            ctx.active_models.remove(&m);
            value
        }
        Type::Intersection(inter) => synth_intersection_json(inter.members(), ctx),
        Type::Union(u) => synth_union_json(u, ctx),
    }
}

fn synth_object_json<'f>(obj: Object<'f>, ctx: &mut SynthCtx<'f>) -> Value {
    if ctx.depth >= MAX_DEPTH || !ctx.consume_node() {
        return Value::Object(Map::new());
    }
    ctx.depth += 1;
    let mut fields: Vec<Field<'f>> = obj.fields().filter(|f| !f.is_read_only()).collect();
    fields.sort_by_key(|f| f.wire_name());

    let mut map = Map::new();
    if !fields.is_empty() {
        for field in fields {
            let val = synth_json(field.ty(), Some(field), ctx);
            map.insert(field.wire_name().to_owned(), val);
        }
    } else if let Some(extra) = obj.extra() {
        let val = synth_json(extra, None, ctx);
        map.insert("key".to_owned(), val);
    }
    ctx.depth -= 1;
    Value::Object(map)
}

fn synth_intersection_json<'f>(members: impl Iterator<Item = Type<'f>>, ctx: &mut SynthCtx<'f>) -> Value {
    if ctx.depth >= MAX_DEPTH || ctx.nodes >= MAX_NODES {
        return Value::Object(Map::new());
    }
    let mut synthesized: Vec<(String, Value)> = members
        .map(|member| {
            let val = synth_json(member, None, ctx);
            let key = serde_json::to_string(&val).unwrap_or_default();
            (key, val)
        })
        .collect();
    // Forge leaves intersection member order unspecified; sort synthesized
    // members canonically so overlapping keys merge deterministically.
    synthesized.sort_by(|a, b| a.0.cmp(&b.0));

    let mut merged: BTreeMap<String, Value> = BTreeMap::new();
    let mut non_objects: Vec<Value> = Vec::new();
    let mut saw_object = false;

    for (_, val) in synthesized {
        match val {
            Value::Object(map) => {
                saw_object = true;
                for (k, v) in map {
                    merged.entry(k).or_insert(v);
                }
            }
            Value::Null => {}
            other => non_objects.push(other),
        }
    }

    if saw_object {
        let map: Map<String, Value> = merged.into_iter().collect();
        return Value::Object(map);
    }
    non_objects.into_iter().next().unwrap_or(Value::Null)
}

fn synth_union_json<'f>(u: forge::Union<'f>, ctx: &mut SynthCtx<'f>) -> Value {
    if let Some(disc) = u.discriminator() {
        let mut cases: Vec<(&str, Type<'f>)> = disc.cases().collect();
        cases.sort_by_key(|&(case_key, _)| case_key);
        if let Some((case_key, case_ty)) = cases.first().copied() {
            let mut val = synth_json(case_ty, None, ctx);
            if let Value::Object(map) = &mut val {
                let mut sorted: BTreeMap<String, Value> = std::mem::take(map).into_iter().collect();
                sorted.insert(disc.wire_name().to_owned(), Value::String(case_key.to_owned()));
                *map = sorted.into_iter().collect();
            }
            return val;
        }
    }

    let mut has_null = false;
    let mut candidates = Vec::new();
    for member in u.members() {
        if matches!(member, Type::Scalar(Scalar::Null)) {
            has_null = true;
        } else {
            candidates.push(member);
        }
    }
    if candidates.is_empty() {
        return Value::Null;
    }
    if has_null && candidates.iter().all(|&m| is_active_model(m, &ctx.active_models)) {
        return Value::Null;
    }
    if candidates.len() == 1 {
        return synth_json(candidates[0], None, ctx);
    }

    // Probe each non-null candidate in an isolated context so rejected
    // branches do not consume the node budget or record unused secrets.
    let mut best: Option<(String, Value, SynthCtx<'f>)> = None;
    for candidate in candidates {
        let mut probe = ctx.clone();
        let val = synth_json(candidate, None, &mut probe);
        let key = serde_json::to_string(&val).unwrap_or_default();
        if best.as_ref().is_none_or(|(best_key, _, _)| key < *best_key) {
            best = Some((key, val, probe));
        }
    }
    if let Some((_, val, probe)) = best {
        *ctx = probe;
        val
    } else {
        Value::Null
    }
}

fn is_active_model<'f>(ty: Type<'f>, active: &HashSet<Model<'f>>) -> bool {
    match ty {
        Type::Model(m) => active.contains(&m),
        Type::Union(u) => {
            let mut saw_non_null = false;
            for member in u.members() {
                if matches!(member, Type::Scalar(Scalar::Null)) {
                    continue;
                }
                saw_non_null = true;
                if !is_active_model(member, active) {
                    return false;
                }
            }
            saw_non_null
        }
        _ => false,
    }
}

fn synth_scalar_json(scalar: Scalar) -> Value {
    match scalar {
        Scalar::Unknown => Value::Object(Map::new()),
        Scalar::Null => Value::Null,
        Scalar::Boolean => Value::Bool(false),
        Scalar::String => Value::String("string".to_owned()),
        Scalar::Uuid => Value::String("00000000-0000-0000-0000-000000000000".to_owned()),
        Scalar::DateTime => Value::String("2026-01-01T00:00:00Z".to_owned()),
        Scalar::Date => Value::String("2026-01-01".to_owned()),
        Scalar::Bytes => Value::String("c3RyaW5n".to_owned()),
        Scalar::Binary => Value::String("<binary>".to_owned()),
        Scalar::Number(n) => {
            if n.is_integer() {
                Value::Number(Number::from(0))
            } else {
                Value::Number(Number::from_f64(0.0).expect("0.0 is finite"))
            }
        }
    }
}

/// Synthesizes a scalar value for path variables, query parameters, headers,
/// form fields, or plain text bodies.
///
/// When `path_wire_name` is `Some`, plain `String` and `Unknown` types render
/// as `<wire_name>` instead of `string`/`value`.
pub(super) fn synth_scalar<'f>(
    ty: Type<'f>,
    field: Option<Field<'f>>,
    path_wire_name: Option<&str>,
    ctx: &mut SynthCtx<'f>,
) -> ScalarVal {
    if let Some(f) = field {
        if f.is_sensitive() {
            let var = ctx.secret_var(f.name(), f.wire_name());
            return ScalarVal::TemplateVar(var);
        }
        if let Some(default) = f.default() {
            return ScalarVal::Literal(normalize_lf(&format_json_scalar(default)));
        }
    }

    match unwrap_type(ty) {
        Type::Scalar(s) => ScalarVal::Literal(synth_scalar_text(s, path_wire_name)),
        Type::Enum(e) => match e.values().next() {
            Some(EnumValue::String(s)) => ScalarVal::Literal(s.to_owned()),
            Some(EnumValue::Number(n)) => ScalarVal::Literal(json_number(n).to_string()),
            Some(EnumValue::Boolean(b)) => ScalarVal::Literal(b.to_string()),
            None => ScalarVal::Literal("value".to_owned()),
        },
        Type::Array(arr) => synth_scalar(arr.elem(), None, path_wire_name, ctx),
        other => {
            let val = synth_json(other, None, ctx);
            ScalarVal::Literal(value_to_scalar_text(&val, path_wire_name))
        }
    }
}

fn synth_scalar_text(scalar: Scalar, path_wire_name: Option<&str>) -> String {
    match scalar {
        Scalar::Unknown => match path_wire_name {
            Some(name) => format!("<{name}>"),
            None => "value".to_owned(),
        },
        Scalar::Null => "null".to_owned(),
        Scalar::Boolean => "false".to_owned(),
        Scalar::String => match path_wire_name {
            Some(name) => format!("<{name}>"),
            None => "string".to_owned(),
        },
        Scalar::Uuid => "00000000-0000-0000-0000-000000000000".to_owned(),
        Scalar::DateTime => "2026-01-01T00:00:00Z".to_owned(),
        Scalar::Date => "2026-01-01".to_owned(),
        Scalar::Bytes => "c3RyaW5n".to_owned(),
        Scalar::Binary => "<binary>".to_owned(),
        Scalar::Number(n) => {
            if n.is_integer() {
                "0".to_owned()
            } else {
                "0.0".to_owned()
            }
        }
    }
}

fn value_to_scalar_text(val: &Value, path_wire_name: Option<&str>) -> String {
    match val {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => match path_wire_name {
            Some(name) => format!("<{name}>"),
            None => serde_json::to_string(val).unwrap_or_else(|_| "value".to_owned()),
        },
    }
}

/// Converts a `forge::Json` value into a `serde_json::Value`, emitting whole
/// finite `f64` values as integers.
pub(super) fn forge_json_to_value(json: &Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Number(json_number(*n)),
        Json::String(s) => Value::String(s.clone()),
        Json::Array(items) => Value::Array(items.iter().map(forge_json_to_value).collect()),
        Json::Object(members) => Value::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), forge_json_to_value(v)))
                .collect(),
        ),
    }
}

/// Formats a `forge::Json` default value as scalar text (unquoted for strings,
/// integer-formatted for whole finite numbers).
pub(super) fn format_json_scalar(json: &Json) -> String {
    match json {
        Json::Null => "null".to_owned(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => json_number(*n).to_string(),
        Json::String(s) => s.clone(),
        Json::Array(_) | Json::Object(_) => {
            serde_json::to_string(&forge_json_to_value(json)).unwrap_or_else(|_| "value".to_owned())
        }
    }
}

/// Converts an `f64` into a `serde_json::Number`, emitting whole finite
/// numbers within IEEE-754 exact integer range as integers (`10`, not `10.0`).
#[expect(clippy::cast_possible_truncation)]
pub(super) fn json_number(n: f64) -> Number {
    if !n.is_finite() {
        return Number::from(0);
    }
    if n.fract() == 0.0 && (-9_007_199_254_740_992.0..=9_007_199_254_740_992.0).contains(&n) {
        return Number::from(n as i64);
    }
    Number::from_f64(n).unwrap_or_else(|| Number::from(0))
}
