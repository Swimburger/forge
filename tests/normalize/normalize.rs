//! `forge::normalize`: a document written with other tools' keys is rewritten
//! to the native ones, and builds the same `Forge` as before.

use forge::normalize::{Options, Report, normalize};
use serde_json::{Value, json};

use super::common::{assert_same, describe, normalized_and_left, operation_with, parse, parse_err, responding_with};

const KEEP: Options = Options {
    leave_old_extensions: true,
};

/// Normalizes `document`, and returns what it became, as it is written, and
/// what was done.
#[track_caller]
fn edit(document: &str, options: &Options) -> (String, Report) {
    let normalized = normalize(document.as_bytes(), options).unwrap();
    let text = normalized
        .json
        .map_or_else(|| document.to_owned(), |json| String::from_utf8(json.to_vec()).unwrap());
    (text, normalized.report)
}

/// Normalizes `document`, and returns what it became, as a value, and what
/// was done.
#[track_caller]
fn run(document: &str, options: &Options) -> (Value, Report) {
    let (text, report) = edit(document, options);
    (serde_json::from_str(&text).unwrap(), report)
}

/// Returns the document at `pointer` of `value`.
#[track_caller]
fn at<'v>(value: &'v Value, pointer: &str) -> &'v Value {
    value
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{pointer} is not in {value}"))
}

/// Every key of the adapters, at the kind of object it is read on.
const ALL: &str = r##"{
  "openapi": "3.0.3", "info": {"title": "t", "version": "1"},
  "paths": {
    "/a/{id}": {"get": {
      "x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-availability": "beta",
      "parameters": [{"name": "id", "in": "path", "required": true,
        "x-fern-parameter-name": "ident", "schema": {"type": "string"}}],
      "responses": {"200": {"description": "ok", "content": {"application/json": {
        "schema": {"$ref": "#/components/schemas/Thing"}}}}}}},
    "/b": {"delete": {
      "x-fern-sdk-group-name": ["b"], "x-fern-sdk-method-name": "gone", "x-fern-ignore": true,
      "responses": {"204": {"description": "ok"}}}}
  },
  "components": {"schemas": {"Thing": {
    "type": "object", "x-fern-type-name": "Widget",
    "properties": {
      "id": {"type": "string", "x-fern-property-name": "widget_id", "x-sensitive": true},
      "legacy": {"type": "string", "x-stainless-skip": true}}}}}
}"##;

#[test]
fn a_normalized_document_builds_the_same_forge() {
    let (normalized, report) = run(ALL, &Options::default());

    assert!(report.changed());
    assert_same(ALL, &normalized.to_string());
}

#[test]
fn every_key_is_rewritten_as_its_native_key() {
    let (normalized, report) = run(ALL, &Options::default());

    let op = at(&normalized, "/paths/~1a~1{id}/get");
    assert_eq!(op["x-forge-group-name"], "a");
    assert_eq!(op["x-forge-method-name"], "get");
    assert_eq!(op["x-forge-availability"], "beta");
    assert_eq!(
        at(&normalized, "/paths/~1a~1{id}/get/parameters/0")["x-forge-parameter-name"],
        "ident"
    );
    assert_eq!(at(&normalized, "/paths/~1b/delete")["x-forge-skip"], true);
    let thing = at(&normalized, "/components/schemas/Thing");
    assert_eq!(thing["x-forge-model-name"], "Widget");
    assert_eq!(thing["properties"]["id"]["x-forge-property-name"], "widget_id");
    assert_eq!(thing["properties"]["id"]["x-forge-sensitive"], true);
    assert_eq!(thing["properties"]["legacy"]["x-forge-skip"], true);

    let text = normalized.to_string();
    for old in ["x-fern-", "x-stainless-", "x-sensitive"] {
        assert!(!text.contains(old), "{old} is left in {text}");
    }
    assert_eq!(report.migrated.values().sum::<usize>(), 11);
    assert_eq!(report.migrated["x-fern-sdk-method-name"], 2);
    assert!(report.conflicts.is_empty());
}

#[test]
fn a_key_is_rewritten_where_it_stands_among_the_keys() {
    let (text, _) = edit(
        &operation_with(
            r#""summary": "s", "x-fern-sdk-method-name": "get", "description": "d", "x-fern-sdk-group-name": ["a"],"#,
        ),
        &Options::default(),
    );

    let at = |key: &str| text.find(key).unwrap();
    assert!(at("summary") < at("x-forge-method-name"));
    assert!(at("x-forge-method-name") < at("description"));
    assert!(at("description") < at("x-forge-group-name"));
}

#[test]
fn keys_that_have_no_native_key_are_left_alone() {
    let document = operation_with(
        r#""x-fern-audiences": ["internal"], "x-fern-enum": 1, "x-stainless-terraform-configurability": "computed",
          "x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get","#,
    );
    let (normalized, _) = run(&document, &Options::default());

    let op = at(&normalized, "/paths/~1a/get");
    assert_eq!(op["x-fern-audiences"], json!(["internal"]));
    assert_eq!(op["x-fern-enum"], 1);
    assert_eq!(op["x-stainless-terraform-configurability"], "computed");
}

#[test]
fn a_key_is_rewritten_only_on_the_objects_it_is_read_on() {
    // `x-fern-ignore` is read on an operation and not on a schema, so a
    // native `x-forge-skip` there would drop a field the document keeps.
    let document = responding_with(
        r#"{"type": "object", "properties": {"id": {"type": "string", "x-fern-ignore": true,
          "x-fern-sdk-method-name": "x"}}}"#,
        "{}",
    );
    let (normalized, report) = run(&document, &Options::default());

    assert!(!report.changed());
    assert_eq!(normalized, serde_json::from_str::<Value>(&document).unwrap());
    assert_same(&document, &normalized.to_string());
}

#[test]
fn a_native_key_is_never_replaced() {
    let document = operation_with(
        r#""x-fern-sdk-method-name": "old", "x-forge-method-name": "new", "x-fern-sdk-group-name": ["a"],"#,
    );
    let (normalized, report) = run(&document, &Options::default());

    let op = at(&normalized, "/paths/~1a/get");
    assert_eq!(op["x-forge-method-name"], "new");
    assert!(op.get("x-fern-sdk-method-name").is_none());
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].location, "/paths/~1a/get");
    assert_eq!(report.conflicts[0].key, "x-fern-sdk-method-name");
    assert_eq!(report.conflicts[0].native, "x-forge-method-name");
}

#[test]
fn a_key_that_says_what_the_native_key_says_is_no_conflict() {
    let document = operation_with(
        r#""x-fern-sdk-method-name": "get", "x-forge-method-name": "get", "x-fern-sdk-group-name": ["a"],"#,
    );
    let (normalized, report) = run(&document, &Options::default());

    assert!(report.conflicts.is_empty());
    assert!(
        at(&normalized, "/paths/~1a/get")
            .get("x-fern-sdk-method-name")
            .is_none()
    );
}

#[test]
fn the_old_keys_can_be_left_beside_the_native_ones() {
    let (normalized, report) = run(ALL, &KEEP);

    let op = at(&normalized, "/paths/~1a~1{id}/get");
    assert_eq!(op["x-fern-sdk-method-name"], "get");
    assert_eq!(op["x-forge-method-name"], "get");
    let thing = at(&normalized, "/components/schemas/Thing");
    assert_eq!(thing["properties"]["id"]["x-sensitive"], true);
    assert_eq!(thing["properties"]["id"]["x-forge-sensitive"], true);
    assert_eq!(report.migrated.values().sum::<usize>(), 11);
    assert_same(ALL, &normalized.to_string());
}

#[test]
fn normalizing_twice_changes_nothing() {
    for options in [Options::default(), KEEP] {
        let (once, _) = edit(ALL, &options);
        let (twice, report) = edit(&once, &options);

        assert!(!report.changed(), "{options:?}");
        assert_eq!(once, twice, "{options:?}");
    }
}

#[test]
fn a_list_of_targets_to_skip_is_left_as_it_is_in_both_modes() {
    let document = responding_with(
        r#"{"type": "object", "properties": {"a": {"type": "string", "x-stainless-skip": ["terraform"]},
          "b": {"type": "string", "x-stainless-skip": false}}}"#,
        "{}",
    );
    let path = "/paths/~1a/get/responses/200/content/application~1json/schema/properties";

    let (normalized, report) = run(&document, &Options::default());

    assert_eq!(
        *at(&normalized, path),
        json!({
            "a": {"type": "string", "x-stainless-skip": ["terraform"]},
            "b": {"type": "string", "x-forge-skip": false}
        })
    );
    assert_eq!(report.migrated["x-stainless-skip"], 1);
    assert_same(&document, &normalized.to_string());

    let (kept, report) = run(&document, &KEEP);

    assert_eq!(
        *at(&kept, path),
        json!({
            "a": {"type": "string", "x-stainless-skip": ["terraform"]},
            "b": {"type": "string", "x-forge-skip": false, "x-stainless-skip": false}
        })
    );
    assert_eq!(report.migrated["x-stainless-skip"], 1);
}

#[test]
fn a_list_of_targets_is_left_beside_a_native_skip_and_is_not_a_conflict() {
    let before = with_property(r#"{"type":"string","x-forge-skip":true,"x-stainless-skip":["t"]}"#);

    for options in [Options::default(), KEEP] {
        let normalized = normalize(before.as_bytes(), &options).unwrap();

        assert!(normalized.json.is_none() && !normalized.report.changed());
        assert!(normalized.report.conflicts.is_empty());
    }
}

#[test]
fn a_list_of_targets_is_not_in_the_way_of_what_is_rewritten_around_it() {
    let before = with_property(r#"{"x-stainless-skip":["t"],"x-sensitive":true,"type":"string"}"#);
    let after = with_property(r#"{"x-stainless-skip":["t"],"x-forge-sensitive":true,"type":"string"}"#);

    let (text, report) = edit(&before, &Options::default());

    assert_eq!(text, after);
    assert_eq!(report.migrated.len(), 1);
}

#[test]
fn x_sensitive_is_rewritten_to_x_forge_sensitive() {
    let document = responding_with(
        r#"{"type": "object", "properties": {"secret": {"type": "string", "x-sensitive": true}}}"#,
        "{}",
    );
    let (normalized, report) = run(&document, &Options::default());

    let secret = at(
        &normalized,
        "/paths/~1a/get/responses/200/content/application~1json/schema/properties/secret",
    );
    assert_eq!(*secret, json!({"type": "string", "x-forge-sensitive": true}));
    assert_eq!(report.migrated["x-sensitive"], 1);
}

#[test]
fn aliases_are_rewritten() {
    let document =
        operation_with(r#""x-forge-aliases": [{"x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "pause"}],"#);
    let (normalized, _) = run(&document, &Options::default());

    assert_eq!(
        *at(&normalized, "/paths/~1a/get/x-forge-aliases/0"),
        json!({"x-forge-group-name": "a", "x-forge-method-name": "pause"})
    );
    assert_same(&document, &normalized.to_string());
}

#[test]
fn schemas_are_found_wherever_forge_reads_them() {
    let skipped = json!({"type": "string", "x-stainless-skip": true});
    let media = |schema: &Value| json!({"text/plain": {"schema": schema}});
    let document = json!({
        "openapi": "3.1.0", "info": {"title": "t", "version": "1"},
        "paths": {"/a": {
            "parameters": [{"name": "p", "in": "query", "x-fern-parameter-name": "q", "schema": skipped}],
            "get": {
                "requestBody": {"content": {"application/json": {"schema": {
                    "allOf": [skipped], "items": skipped, "additionalProperties": skipped,
                    "prefixItems": [skipped], "not": skipped, "$defs": {"d": skipped}}}}},
                "responses": {"200": {"description": "ok", "content": media(&skipped)}}}}},
        "components": {
            "parameters": {"p": {"name": "p", "in": "query", "content": media(&skipped)}},
            "requestBodies": {"r": {"content": media(&skipped)}},
            "responses": {"s": {"description": "ok", "content": media(&skipped)}},
            "pathItems": {"i": {"get": {"responses": {"200": {"description": "ok", "content": media(&skipped)}}}}}}
    });
    let (normalized, report) = run(&document.to_string(), &Options::default());

    assert!(!normalized.to_string().contains("x-stainless-skip"));
    assert_eq!(report.migrated["x-stainless-skip"], 12);
    assert_eq!(report.migrated["x-fern-parameter-name"], 1);
}

#[test]
fn a_conflict_says_where_it_is_as_a_json_pointer() {
    let document = r#"{"openapi": "3.0.3", "info": {"title": "t", "version": "1"},
      "paths": {"/a/{id}": {"get": {"parameters": [
        {"name": "id", "in": "path", "x-fern-parameter-name": "a", "x-forge-parameter-name": "b",
          "schema": {"type": "string"}}],
        "responses": {"204": {"description": "ok"}}}}}}"#;
    let (_, report) = run(document, &Options::default());

    assert_eq!(report.conflicts[0].location, "/paths/~1a~1{id}/get/parameters/0");
}

/// A document with a key to rewrite in each of the ways a document is written,
/// and everything else that a rewrite must not touch.
const AWKWARD: &str = concat!(
    "{\r\n",
    "\t\"openapi\" : \"3.0.3\",\r\n",
    "\t\"info\":{\"title\":\"caf\\u00e9 \\/ t\",\"version\":\"1\"},\r\n",
    "\t\"paths\":{\"/a\":{\"get\":{ \"x-fern-sdk-method-name\" :\"get\" ,\"x-fern-sdk-group-name\":[ \"a\" ],\r\n",
    "\t\t\"description\":\"\\u00e9\\n\",\"responses\":{\"200\":{\"description\":\"ok\",\"content\":{\"application/json\":{",
    "\"schema\":{\"type\":\"object\",\"properties\":{\r\n",
    "\t\t\t\"a\":{\"type\":\"number\",\"example\":116.11599994199997,\"x-fern-property-name\":\"b\"},\r\n",
    "\t\t\t\"c\":{\"type\":\"number\",\"example\":0.000005,\"minimum\":1E5,\"maximum\":1.0,\"x-sensitive\":true}",
    "}}}}}}}}}\r\n",
    "}\r\n",
    "\r\n",
);

#[test]
fn nothing_but_the_keys_that_are_rewritten_changes() {
    let (text, report) = edit(AWKWARD, &Options::default());

    // The group is the one value that is written again, as a dotted string.
    let want = AWKWARD
        .replace("\"x-fern-sdk-method-name\"", "\"x-forge-method-name\"")
        .replace("\"x-fern-sdk-group-name\":[ \"a\" ]", "\"x-forge-group-name\":\"a\"")
        .replace("\"x-fern-property-name\"", "\"x-forge-property-name\"")
        .replace("\"x-sensitive\"", "\"x-forge-sensitive\"");
    assert_eq!(text, want);
    assert_eq!(report.migrated.values().sum::<usize>(), 4);
    for kept in [
        "116.11599994199997",
        "0.000005",
        "1E5",
        "1.0",
        "caf\\u00e9 \\/ t",
        "\\u00e9\\n",
    ] {
        assert!(text.contains(kept), "{kept}");
    }
}

/// Returns a document with one schema in `components.schemas` that has the
/// property `p` written as `property`.
fn with_property(property: &str) -> String {
    format!(
        r#"{{"openapi":"3.0.3","info":{{"title":"t","version":"1"}},"paths":{{}},"components":{{"schemas":{{"S":{{"type":"object","properties":{{"p":{property}}}}}}}}}}}"#
    )
}

#[test]
fn a_removed_key_takes_its_comma_and_leaves_the_others_as_they_were() {
    // Each key that is removed has the native key that says what it says
    // beside it, wherever that is.
    for (before, after) in [
        (
            r#"{"type":"string","x-forge-sensitive":true,"x-sensitive":false}"#,
            r#"{"type":"string","x-forge-sensitive":true}"#,
        ),
        (
            r#"{"x-sensitive":false,"x-forge-sensitive":true,"type":"string"}"#,
            r#"{"x-forge-sensitive":true,"type":"string"}"#,
        ),
        (
            r#"{"type":"string","x-sensitive":false,"x-forge-sensitive":true,"format":"uri"}"#,
            r#"{"type":"string","x-forge-sensitive":true,"format":"uri"}"#,
        ),
        (
            r#"{"x-forge-sensitive":true,"x-sensitive":false}"#,
            r#"{"x-forge-sensitive":true}"#,
        ),
        (
            r#"{"x-sensitive":false,"x-forge-sensitive":true}"#,
            r#"{"x-forge-sensitive":true}"#,
        ),
        (
            r#"{"x-sensitive":false,"x-stainless-skip":true,"x-forge-sensitive":true,"x-forge-skip":false,"type":"string"}"#,
            r#"{"x-forge-sensitive":true,"x-forge-skip":false,"type":"string"}"#,
        ),
        (
            r#"{"type":"string","x-sensitive":false,"x-forge-sensitive":true,"x-stainless-skip":true,"x-forge-skip":false}"#,
            r#"{"type":"string","x-forge-sensitive":true,"x-forge-skip":false}"#,
        ),
    ] {
        let (text, _) = edit(&with_property(before), &Options::default());

        assert_eq!(text, with_property(after), "{before}");
    }
}

#[test]
fn a_removed_key_leaves_the_whitespace_of_the_others() {
    let pretty = |members: &str| with_property(&format!("{{{members}}}"));
    for (before, after) in [
        (
            "\n      \"type\": \"string\",\n      \"x-forge-sensitive\": true,\n      \"x-sensitive\":\n        false\n    ",
            "\n      \"type\": \"string\",\n      \"x-forge-sensitive\": true\n    ",
        ),
        (
            "\n      \"x-sensitive\": false,\n      \"x-forge-sensitive\": true,\n      \"type\": \"string\"\n    ",
            "\n      \"x-forge-sensitive\": true,\n      \"type\": \"string\"\n    ",
        ),
        (
            "\n\t\"type\": \"string\",\n\t\"x-sensitive\": false,\n\t\"x-forge-sensitive\": true,\n\t\"format\": \"uri\"\n",
            "\n\t\"type\": \"string\",\n\t\"x-forge-sensitive\": true,\n\t\"format\": \"uri\"\n",
        ),
    ] {
        let (text, _) = edit(&pretty(before), &Options::default());

        assert_eq!(text, pretty(after), "{before:?}");
    }
}

#[test]
fn a_key_that_is_left_is_preceded_by_the_native_key_written_the_same_way() {
    for (before, after) in [
        (
            r#"{"type":"string","x-fern-property-name":"a"}"#,
            r#"{"type":"string","x-forge-property-name":"a","x-fern-property-name":"a"}"#,
        ),
        (
            "{\n      \"x-fern-property-name\" : [\"a\"],\n      \"type\": \"string\"\n    }",
            "{\n      \"x-forge-property-name\" : [\"a\"],\n      \"x-fern-property-name\" : [\"a\"],\n      \"type\": \"string\"\n    }",
        ),
    ] {
        let (text, _) = edit(&with_property(before), &KEEP);

        assert_eq!(text, with_property(after), "{before}");
    }
}

#[test]
fn leaving_the_old_keys_only_adds_lines() {
    let pretty = serde_json::to_string_pretty(&serde_json::from_str::<Value>(ALL).unwrap()).unwrap();

    let (text, report) = edit(&pretty, &KEEP);

    assert!(report.changed());
    let mut original = pretty.lines().peekable();
    for line in text.lines() {
        if original.peek() == Some(&line) {
            original.next();
        }
    }
    assert_eq!(original.next(), None, "a line of the document is changed or gone");
    assert!(text.lines().count() > pretty.lines().count());
}

#[test]
fn a_group_path_is_written_as_the_names_joined_by_dots() {
    let document = operation_with(r#""x-fern-sdk-group-name": ["a", "b", "c"], "x-fern-sdk-method-name": "get","#);

    let (normalized, _) = run(&document, &Options::default());

    assert_eq!(at(&normalized, "/paths/~1a/get")["x-forge-group-name"], "a.b.c");
    assert_same(&document, &normalized.to_string());
}

#[test]
fn a_group_path_written_over_several_lines_becomes_one_string() {
    let before = operation_with(
        "\n    \"x-fern-sdk-group-name\": [\n      \"a\",\n      \"b\"\n    ],\n    \"x-fern-sdk-method-name\": \"get\",",
    );
    let after = operation_with("\n    \"x-forge-group-name\": \"a.b\",\n    \"x-forge-method-name\": \"get\",");

    let (text, _) = edit(&before, &Options::default());

    assert_eq!(text, after);
}

#[test]
fn a_group_path_that_is_a_string_is_written_as_it_was() {
    let document = operation_with(r#""x-fern-sdk-group-name": "a.b", "x-fern-sdk-method-name": "get","#);

    let (text, _) = edit(&document, &Options::default());

    assert_eq!(
        text,
        operation_with(r#""x-forge-group-name": "a.b", "x-forge-method-name": "get","#)
    );
}

#[test]
fn a_group_path_with_no_names_is_the_empty_string() {
    let (normalized, _) = run(
        &operation_with(r#""x-fern-sdk-group-name": [], "x-fern-sdk-method-name": "get","#),
        &Options::default(),
    );

    assert_eq!(at(&normalized, "/paths/~1a/get")["x-forge-group-name"], "");
}

#[test]
fn a_group_path_a_dotted_string_cannot_say_is_kept_as_an_array() {
    for names in [r#"["a.b", "c"]"#, r#"["a", ""]"#, r#"["a", 1]"#] {
        let document = operation_with(&format!(
            r#""x-fern-sdk-group-name": {names}, "x-fern-sdk-method-name": "get","#
        ));

        let (text, _) = edit(&document, &Options::default());

        assert_eq!(
            text,
            operation_with(&format!(
                r#""x-forge-group-name": {names}, "x-forge-method-name": "get","#
            )),
            "{names}"
        );
    }
}

#[test]
fn a_fern_group_with_a_dot_in_a_name_stays_an_array_byte_for_byte() {
    // Written over several lines, so that each byte of the array is checked.
    let before = operation_with(
        "\n    \"x-fern-sdk-group-name\": [\n      \"a.b\",\n      \"c\"\n    ],\n    \"x-fern-sdk-method-name\": \"get\",",
    );
    let after = operation_with(
        "\n    \"x-forge-group-name\": [\n      \"a.b\",\n      \"c\"\n    ],\n    \"x-forge-method-name\": \"get\",",
    );

    let (text, _) = edit(&before, &Options::default());

    assert_eq!(text, after);
    assert!(!text.contains("a.b.c"), "the names were joined");
}

#[test]
fn a_group_path_that_is_joined_is_read_back_as_the_same_path() {
    for names in [
        vec!["a"],
        vec!["a", "b"],
        vec!["zero-trust", "dlp", "custom-prompt-topics"],
        vec!["with_underscore", "v2", "2fa"],
        vec!["caf\u{e9}", "\u{4e2d}\u{6587}"],
        vec!["a", "b", "c", "d", "e", "f", "g"],
    ] {
        let array = format!(
            "[{}]",
            names
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let document = operation_with(&format!(
            r#""x-fern-sdk-group-name": {array}, "x-fern-sdk-method-name": "get","#
        ));

        let (normalized, _) = run(&document, &Options::default());

        let joined = at(&normalized, "/paths/~1a/get")["x-forge-group-name"]
            .as_str()
            .unwrap();
        assert_eq!(joined, names.join("."), "{names:?}");
        assert_same(&document, &normalized.to_string());
    }
}

#[test]
fn a_group_with_a_dot_in_a_name_is_still_the_same_groups() {
    let document = operation_with(r#""x-fern-sdk-group-name": ["a.b", "c"], "x-fern-sdk-method-name": "get","#);

    let (text, _) = edit(&document, &Options::default());

    assert_same(&document, &text);
    // Two names, `a.b` and `c`. Joined into `"a.b.c"` they would be three, so
    // that is the mistake this keeps from being made.
    assert!(
        describe(&parse(&text)).contains("group a_b.c"),
        "{}",
        describe(&parse(&text))
    );
    let joined = operation_with(r#""x-forge-group-name": "a.b.c", "x-forge-method-name": "get","#);
    assert_ne!(describe(&parse(&joined)), describe(&parse(&text)));
}

#[test]
fn leaving_the_old_key_copies_a_group_with_a_dot_as_an_array() {
    let before = operation_with(r#""x-fern-sdk-group-name": ["a.b", "c"], "x-fern-sdk-method-name": "get","#);
    let after = operation_with(
        r#""x-forge-group-name": ["a.b", "c"],"x-fern-sdk-group-name": ["a.b", "c"], "x-forge-method-name": "get", "x-fern-sdk-method-name": "get","#,
    );

    let (text, _) = edit(&before, &KEEP);

    assert_eq!(text, after);
}

#[test]
fn only_the_groups_with_a_dot_in_a_name_stay_arrays() {
    let document = r#"{"openapi": "3.0.3", "info": {"title": "t", "version": "1"}, "paths": {
      "/a": {"get": {"x-fern-sdk-group-name": ["a.b", "c"], "x-fern-sdk-method-name": "get", "responses": {"204": {"description": "ok"}}}},
      "/b": {"get": {"x-fern-sdk-group-name": ["x", "y"], "x-fern-sdk-method-name": "get", "responses": {"204": {"description": "ok"}}}},
      "/c": {"get": {"x-fern-sdk-group-name": ["p", "q.r"], "x-fern-sdk-method-name": "get", "responses": {"204": {"description": "ok"}}}}}}"#;

    let (normalized, _) = run(document, &Options::default());

    assert_eq!(
        at(&normalized, "/paths/~1a/get")["x-forge-group-name"],
        json!(["a.b", "c"])
    );
    assert_eq!(at(&normalized, "/paths/~1b/get")["x-forge-group-name"], "x.y");
    assert_eq!(
        at(&normalized, "/paths/~1c/get")["x-forge-group-name"],
        json!(["p", "q.r"])
    );
    assert_same(document, &normalized.to_string());
}

#[test]
fn an_alias_with_a_dot_in_a_group_name_stays_an_array() {
    let document = operation_with(
        r#""x-forge-aliases": [{"x-fern-sdk-group-name": ["a.b", "c"], "x-fern-sdk-method-name": "pause"}, {"x-fern-sdk-group-name": ["x", "y"], "x-fern-sdk-method-name": "stop"}],"#,
    );

    let (normalized, _) = run(&document, &Options::default());

    assert_eq!(
        at(&normalized, "/paths/~1a/get/x-forge-aliases/0")["x-forge-group-name"],
        json!(["a.b", "c"])
    );
    assert_eq!(
        at(&normalized, "/paths/~1a/get/x-forge-aliases/1")["x-forge-group-name"],
        "x.y"
    );
    assert_same(&document, &normalized.to_string());
}

#[test]
fn a_group_with_a_dot_is_not_the_same_as_the_dotted_string_beside_it() {
    // `["a.b", "c"]` is two groups, and `"a.b.c"` is three, so this is a
    // conflict, and the same array beside itself is none.
    let different = operation_with(
        r#""x-fern-sdk-group-name": ["a.b", "c"], "x-forge-group-name": "a.b.c", "x-forge-method-name": "get","#,
    );
    let same = operation_with(
        r#""x-fern-sdk-group-name": ["a.b", "c"], "x-forge-group-name": ["a.b", "c"], "x-forge-method-name": "get","#,
    );

    assert_eq!(edit(&different, &Options::default()).1.conflicts.len(), 1);
    assert!(edit(&same, &Options::default()).1.conflicts.is_empty());
}

#[test]
fn leaving_the_old_key_writes_the_native_one_as_a_string_and_keeps_the_array() {
    let before = operation_with(r#""x-fern-sdk-group-name": ["a", "b"], "x-fern-sdk-method-name": "get","#);
    let after = operation_with(
        r#""x-forge-group-name": "a.b","x-fern-sdk-group-name": ["a", "b"], "x-forge-method-name": "get", "x-fern-sdk-method-name": "get","#,
    );

    let (text, _) = edit(&before, &KEEP);

    assert_eq!(text, after);
}

#[test]
fn a_native_group_that_says_the_same_in_the_other_form_is_no_conflict() {
    for (native, old) in [(r#""a.b""#, r#"["a", "b"]"#), (r#"["a", "b"]"#, r#""a.b""#)] {
        let document = operation_with(&format!(
            r#""x-fern-sdk-group-name": {old}, "x-forge-group-name": {native}, "x-forge-method-name": "get","#
        ));

        let (_, report) = edit(&document, &Options::default());

        assert!(report.conflicts.is_empty(), "{native} and {old}");
    }
}

#[test]
fn a_native_group_that_says_something_else_is_a_conflict() {
    let document = operation_with(
        r#""x-fern-sdk-group-name": ["a", "b"], "x-forge-group-name": "a.c", "x-forge-method-name": "get","#,
    );

    let (normalized, report) = run(&document, &Options::default());

    assert_eq!(at(&normalized, "/paths/~1a/get")["x-forge-group-name"], "a.c");
    assert_eq!(report.conflicts.len(), 1);
}

#[test]
fn a_key_written_twice_is_left_as_it_is_and_the_others_are_rewritten() {
    let document = operation_with(
        r#""x-fern-sdk-method-name": "a", "x-fern-sdk-method-name": "b", "x-fern-sdk-group-name": ["a"],"#,
    );

    let (text, report) = edit(&document, &Options::default());

    assert_eq!(
        text,
        operation_with(r#""x-fern-sdk-method-name": "a", "x-fern-sdk-method-name": "b", "x-forge-group-name": "a","#)
    );
    assert_eq!(report.migrated.len(), 1);
    let [left] = &report.left[..] else {
        panic!("{:?}", report.left)
    };
    assert_eq!(
        (left.location.as_str(), left.key.as_str(), left.why),
        (
            "/paths/~1a/get",
            "x-fern-sdk-method-name",
            "is written twice in this object"
        )
    );
}

/// `forge` reads no native key, nor any other key it knows, that is written
/// twice in an object it reads. A key of another tool is an ordinary extension
/// to it, which it takes as the last value written.
#[test]
fn forge_rejects_a_key_written_twice_in_an_object() {
    for keys in [
        r#""x-forge-method-name": "a", "x-forge-method-name": "b", "x-forge-group-name": "g","#,
        r#""x-forge-method-name": "a", "x-forge-method-name": "b", "x-fern-sdk-method-name": "c", "x-forge-group-name": "g","#,
        r#""description": "a", "description": "b", "x-forge-method-name": "a", "x-forge-group-name": "g","#,
    ] {
        let err = parse_err(&operation_with(keys));

        assert!(err.contains("duplicate field"), "{keys}: {err}");
    }
}

#[test]
fn a_native_key_written_twice_beside_a_key_that_is_rewritten_leaves_that_key_in_both_modes() {
    // Which of the two is the native key that is beside is not for a rewrite
    // to say, so the key beside them is left as it is, and `forge` says what
    // it makes of a native key written twice.
    for (keys, name) in [
        (
            r#""x-forge-method-name": "a", "x-forge-method-name": "b", "x-fern-sdk-method-name": "c", "x-forge-group-name": "g","#,
            "x-forge-method-name",
        ),
        (
            r#""x-fern-sdk-method-name": "c", "x-forge-method-name": "a", "x-forge-group-name": "g", "x-forge-method-name": "b","#,
            "x-forge-method-name",
        ),
        // One of them written with an escape is the same key.
        (
            r#""x-fern-ignore": true, "x-forge-skip": true, "x\u002dforge-skip": false,"#,
            "x-forge-skip",
        ),
    ] {
        let document = operation_with(keys);
        for options in [Options::default(), KEEP] {
            let normalized = normalize(document.as_bytes(), &options).unwrap();

            assert!(normalized.json.is_none() && !normalized.report.changed(), "{keys}");
            let [left] = &normalized.report.left[..] else {
                panic!("{keys}: {:?}", normalized.report.left)
            };
            assert_eq!((left.key.as_str(), left.why), (name, "is written twice in this object"));
        }
    }
}

#[test]
fn a_native_key_written_twice_with_no_key_to_rewrite_beside_it_is_left_as_it_is() {
    // Nothing is rewritten there, so nothing is read there: it is what
    // `forge` says when it reads the document, which `normalize` is not.
    let document = operation_with(
        r#""x-forge-method-name": "a", "x-forge-method-name": "b", "x-forge-group-name": "g", "description": "x", "description": "y","#,
    );

    for options in [Options::default(), KEEP] {
        let normalized = normalize(document.as_bytes(), &options).unwrap();

        assert!(normalized.json.is_none() && !normalized.report.changed());
    }
}

#[test]
fn a_native_key_written_twice_in_one_object_does_not_stop_the_others_being_rewritten() {
    let document = r#"{"openapi": "3.0.3", "info": {"title": "t", "version": "1"}, "paths": {},
          "components": {"schemas": {
            "Dup": {"x-forge-skip": true, "x-forge-skip": false},
            "Fine": {"x-sensitive": true}}}}"#;

    let (text, report) = edit(document, &Options::default());

    assert!(text.contains(r#""Fine": {"x-forge-sensitive": true}"#), "{text}");
    assert!(
        text.contains(r#""Dup": {"x-forge-skip": true, "x-forge-skip": false}"#),
        "{text}"
    );
    assert_eq!(report.migrated["x-sensitive"], 1);
}

#[test]
fn a_key_written_twice_is_left_with_where_a_parser_that_refused_it_would_be() {
    // After the name of the second copy, in bytes, on its line.
    let document = "{\"openapi\":\"3.0.3\",\"info\":{\"title\":\"t\",\"version\":\"1\"},\"paths\":{\"/a\":{\"get\":{\n  \"x-fern-sdk-method-name\": \"a\",\n  \"x-fern-sdk-method-name\": \"b\",\n  \"x-fern-sdk-method-name\": \"c\",\n  \"responses\": {\"204\": {\"description\": \"ok\"}}}}}}";

    let normalized = normalize(document.as_bytes(), &Options::default()).unwrap();

    let [left] = &normalized.report.left[..] else {
        panic!("{:?}", normalized.report.left)
    };
    assert!(left.written_twice());
    assert_eq!((left.line, left.column), (3, 26));
}

#[test]
fn a_value_that_cannot_be_read_is_left_as_it_is_and_does_not_stop_the_others() {
    // A lone surrogate is JSON that a value cannot be made of.
    let document = operation_with(
        r#""x-fern-sdk-method-name": "\ud800", "x-fern-sdk-group-name": ["a"], "x-fern-ignore": false,"#,
    );

    let (text, left) = normalized_and_left(&document);

    assert!(text.contains(r#""x-fern-sdk-method-name": "\ud800""#), "{text}");
    assert!(
        text.contains(r#""x-forge-group-name": "a""#) && text.contains(r#""x-forge-skip": false"#),
        "{text}"
    );
    assert_eq!(
        left,
        [(
            "/paths/~1a/get".to_owned(),
            "x-fern-sdk-method-name".to_owned(),
            "has a value that cannot be read"
        )]
    );
}

#[test]
fn a_native_key_beside_one_that_is_rewritten_that_cannot_be_read_leaves_both() {
    let document = operation_with(r#""x-forge-method-name": "\ud800", "x-fern-sdk-method-name": "get","#);

    for options in [Options::default(), KEEP] {
        let normalized = normalize(document.as_bytes(), &options).unwrap();

        assert!(normalized.json.is_none(), "{options:?}");
        let [left] = &normalized.report.left[..] else {
            panic!("{:?}", normalized.report.left)
        };
        assert_eq!(
            (left.key.as_str(), left.why),
            ("x-forge-method-name", "has a value that cannot be read")
        );
    }
}

#[test]
fn a_key_that_cannot_be_read_is_no_key_and_does_not_stop_the_others() {
    let document = operation_with(r#""x\ud800": 1, "x-fern-sdk-group-name": ["a"],"#);

    let (text, left) = normalized_and_left(&document);

    assert!(
        text.contains(r#""x\ud800": 1"#) && text.contains(r#""x-forge-group-name": "a""#),
        "{text}"
    );
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn a_key_written_with_an_escape_is_rewritten_as_the_key_it_is() {
    let before = operation_with(r#""x-fern-sdk-group-name": ["a"], "x\u002dfern-sdk-method-name": "get","#);
    let after = operation_with(r#""x-forge-group-name": "a", "x-forge-method-name": "get","#);

    let (text, _) = edit(&before, &Options::default());

    assert_eq!(text, after);
    assert_same(&before, &text);
}

#[test]
fn a_container_written_with_an_escape_is_followed() {
    let before = responding_with(
        r#"{"type": "object", "propert\u0069es": {"id": {"type": "string", "x-sensitive": true}}}"#,
        "{}",
    );

    let (text, report) = edit(&before, &Options::default());

    assert_eq!(text, before.replace("\"x-sensitive\"", "\"x-forge-sensitive\""));
    assert_eq!(report.migrated["x-sensitive"], 1);
}

#[test]
fn mangled_documents_do_not_panic_and_what_is_written_is_json() {
    let junk = [b'"', b'{', b'}', b'[', b']', b',', b':', b'\\', 0xFF, b'x', b' '];
    let mut documents: Vec<Vec<u8>> = Vec::new();
    for end in 0..=ALL.len() {
        documents.push(ALL.as_bytes()[..end].to_vec());
    }
    for at in 0..ALL.len() {
        let mut without = ALL.as_bytes().to_vec();
        without.remove(at);
        documents.push(without);
        for byte in junk {
            let mut changed = ALL.as_bytes().to_vec();
            changed[at] = byte;
            documents.push(changed);
        }
    }

    for document in &documents {
        for options in [Options::default(), KEEP] {
            if let Ok(normalized) = normalize(document, &options)
                && let Some(json) = normalized.json
            {
                assert!(
                    serde_json::from_slice::<Value>(&json.to_vec()).is_ok(),
                    "{}",
                    String::from_utf8_lossy(document)
                );
            }
        }
    }
}

/// A writer that takes a few bytes of what it is given each time, and is
/// interrupted now and then, as a pipe is.
struct Trickle {
    taken: Vec<u8>,
    calls: usize,
}

impl std::io::Write for Trickle {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.calls += 1;
        // A writer that is given the same bytes again and again has to fail
        // the test, and not fill the memory.
        assert!(self.taken.len() < 1 << 22, "more was written than the document has");
        if self.calls.is_multiple_of(5) {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let n = bytes.len().min(7);
        self.taken.extend_from_slice(&bytes[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A document with `n` schemas that each have a key to rewrite, which is more
/// parts to write than are handed to the system at once.
fn many_schemas(n: usize) -> String {
    let schemas: Vec<String> = (0..n)
        .map(|i| format!(r#""s{i}": {{"type": "string", "x-sensitive": true}}"#))
        .collect();
    format!(
        r#"{{"openapi": "3.0.3", "info": {{"title": "t", "version": "1"}}, "paths": {{}},
          "components": {{"schemas": {{{}}}}}}}"#,
        schemas.join(",\n")
    )
}

#[test]
fn an_edited_document_is_written_whole_however_much_the_writer_takes() {
    for n in [1, 300, 700] {
        let document = many_schemas(n);
        let normalized = normalize(document.as_bytes(), &Options::default()).unwrap();
        let edited = normalized.json.unwrap();
        let expected = edited.to_vec();

        let mut trickle = Trickle {
            taken: Vec::new(),
            calls: 0,
        };
        edited.write_to(&mut trickle).unwrap();
        let mut whole = Vec::new();
        edited.write_to(&mut whole).unwrap();

        assert_eq!(trickle.taken, expected, "{n} schemas, a few bytes at a time");
        assert_eq!(whole, expected, "{n} schemas");
        let text = String::from_utf8(expected).unwrap();
        assert_eq!(text.matches("\"x-forge-sensitive\"").count(), n);
        assert!(!text.contains("\"x-sensitive\""));
        assert_same(&document, &text);
    }
}

#[test]
fn a_document_that_ends_in_the_middle_of_an_escape_is_an_error_and_not_a_panic() {
    // Where a key or a value is passed over, and where it is read.
    for document in [
        r#"{"a": "x\"#,
        r#"{"x-fern-ignore": "x\"#,
        r#"{"paths": {"/a": {"get": {"x-fern-sdk-method-name": "x\"#,
        r#"{"a\"#,
        r#"{"paths": {"/a\"#,
    ] {
        assert!(
            normalize(document.as_bytes(), &Options::default()).is_err(),
            "{document}"
        );
    }
}

#[test]
fn schemas_nested_without_end_are_an_error_and_not_a_stack_overflow() {
    let depth = 20_000;
    let document = format!(
        r#"{{"components": {{"schemas": {{"a": {}{}{}}}}}}}"#,
        r#"{"properties": {"a": "#.repeat(depth),
        "{}",
        "}}".repeat(depth),
    );

    assert!(normalize(document.as_bytes(), &Options::default()).is_err());
}

#[test]
fn values_that_are_not_read_may_be_nested_as_deep_as_they_like() {
    let depth = 200_000;
    let document = format!(r#"{{"x-other": {}{}}}"#, "[".repeat(depth), "]".repeat(depth));

    let normalized = normalize(document.as_bytes(), &Options::default()).unwrap();

    assert!(!normalized.report.changed());
}

#[test]
fn a_document_that_is_broken_where_it_is_edited_is_blamed_for_it() {
    // The comma that is missing is between two keys that are rewritten, and
    // the scan does not mind, so it is what is written that is not JSON.
    let document = operation_with(r#""x-fern-sdk-method-name": "get" "x-fern-sdk-group-name": ["a"],"#);

    let err = normalize(document.as_bytes(), &Options::default())
        .unwrap_err()
        .to_string();

    assert!(err.contains("expected `,` or `}`"), "{err}");
    assert!(err.contains("line 2"), "{err}");
}

/// The document is checked as a whole, wherever it is walked and wherever it
/// is not, here `info`, and whether there is something to rewrite in it or not.
#[test]
fn a_document_that_is_not_json_is_an_error_where_there_is_nothing_to_rewrite_in_it() {
    let document = br#"{"info": {"title": "t" "version": 1}, "paths": {}}"#;

    let err = normalize(document, &Options::default()).unwrap_err().to_string();

    assert!(err.contains("expected `,` or `}`"), "{err}");
}

/// A long document is checked on another thread, which is the same check.
#[test]
fn a_long_document_is_checked_and_rewritten_as_a_short_one_is() {
    let padding = "p".repeat(300_000);
    let valid = operation_with(r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get","#)
        .replace("\"t\"", &format!("\"{padding}\""));
    let broken = valid.replace("\"version\"", "\"version\" 1,\"x\"");
    let not_utf8 = {
        let mut bytes = valid.clone().into_bytes();
        let at = bytes.windows(5).position(|window| window == b"ppppp").unwrap();
        bytes[at] = 0xFF;
        bytes
    };

    let (text, report) = edit(&valid, &Options::default());

    assert!(valid.len() > 1 << 18 && report.changed());
    assert!(text.contains("x-forge-group-name") && !text.contains("x-fern-"));
    assert_same(&valid, &text);
    assert!(normalize(broken.as_bytes(), &Options::default()).is_err());
    let err = normalize(&not_utf8, &Options::default()).unwrap_err().to_string();
    assert!(err.contains("not UTF-8"), "{err}");
}

#[test]
fn a_document_that_is_cut_off_where_it_is_walked_is_an_error() {
    let err = normalize(br#"{"openapi": "3.0.3", "paths": {"#, &Options::default())
        .unwrap_err()
        .to_string();

    assert!(err.contains("EOF while parsing"), "{err}");
}

#[test]
fn a_document_that_is_not_utf8_is_an_error_when_there_is_something_to_rewrite_in_it() {
    let mut document =
        operation_with(r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get","#).into_bytes();
    let at = document.windows(5).position(|window| window == b"3.0.3").unwrap();
    document[at] = 0xFF;

    let err = normalize(&document, &Options::default()).unwrap_err().to_string();

    assert!(err.contains("not UTF-8"), "{err}");
}

/// Needs the released aggregate: `FORGE_SPEC=openapi.forge.json cargo test
/// --release --test normalize aggregate -- --ignored --nocapture`.
///
/// Both modes build the same `Forge`, apart from the extensions the old keys
/// that are left are, and it has the methods of the document. `forge` does not
/// build it from the document as it is written: the keys of other tools are not
/// read, so it is `forge normalize` that says what they mean.
#[test]
#[ignore = "needs FORGE_SPEC, the path of a large document"]
fn the_aggregate_builds_the_same_forge_in_both_modes_and_only_when_normalized() {
    let path = std::env::var("FORGE_SPEC").expect("FORGE_SPEC is the path of the document");
    let json = std::fs::read(&path).unwrap();
    let unread = super::dump::dump(&serde_json::from_slice(&json).unwrap());

    let mut dumps = Vec::new();
    for options in [Options::default(), KEEP] {
        let normalized = normalize(&json, &options).unwrap();
        dumps.push(super::dump::dump(
            &serde_json::from_slice(&normalized.json.unwrap().to_vec()).unwrap(),
        ));

        let report = normalized.report;
        eprintln!(
            "{options:?}: {} keys, {} conflicts",
            report.migrated.values().sum::<usize>(),
            report.conflicts.len()
        );
        for (key, count) in &report.migrated {
            eprintln!("  {key}: {count}");
        }
        assert!(report.conflicts.is_empty(), "{options:?}: the document has conflicts");
    }

    // The keys that are left are extensions to `forge`, which plugins see.
    let extensions = |dump: &str| {
        dump.lines()
            .filter(|line| line.trim_start().starts_with("extension "))
            .count()
    };
    let model = |dump: &str| -> String {
        dump.lines()
            .filter(|line| !line.trim_start().starts_with("extension "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        extensions(&dumps[1]) > extensions(&dumps[0]),
        "the old keys that are left are not extensions: {} against {}",
        extensions(&dumps[1]),
        extensions(&dumps[0])
    );
    if model(&dumps[0]) != model(&dumps[1]) {
        let (a, b) = (model(&dumps[0]), model(&dumps[1]));
        let differing = a.lines().zip(b.lines()).find(|(a, b)| a != b);
        panic!("the two modes build different Forges, first at:\n{differing:?}");
    }
    assert!(
        dumps[0].len() > 10 * unread.len(),
        "normalizing did not give the document its methods: {} bytes, against {} unread",
        dumps[0].len(),
        unread.len()
    );
}
