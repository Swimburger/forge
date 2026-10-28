//! Fern's `x-fern-*` extensions: each is read as the native extension that
//! says the same, and as nothing else.

use super::common::{
    assert_same, describe, normalized, normalized_and_left, operation_with, parse, parse_err, parse_normalized,
    responding_with,
};

#[test]
fn method_group_and_availability_are_read_as_native() {
    assert_same(
        &operation_with(
            r#""x-fern-sdk-group-name": ["accounts"], "x-fern-sdk-method-name": "list", "x-fern-availability": "beta","#,
        ),
        &operation_with(
            r#""x-forge-group-name": ["accounts"], "x-forge-method-name": "list", "x-forge-availability": "beta","#,
        ),
    );
}

#[test]
fn every_availability_is_read() {
    for availability in ["alpha", "beta", "preview", "generally-available", "deprecated"] {
        assert_same(
            &operation_with(&format!(
                r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-availability": "{availability}","#
            )),
            &operation_with(&format!(
                r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "{availability}","#
            )),
        );
    }
}

/// As `x-forge-group-name` reads it, which Fern does not: Fern reads the string
/// as one group.
#[test]
fn a_group_written_as_a_string_is_split_on_dots() {
    assert_same(
        &operation_with(r#""x-fern-sdk-group-name": "a.b.c", "x-fern-sdk-method-name": "get","#),
        &operation_with(r#""x-forge-group-name": "a.b.c", "x-forge-method-name": "get","#),
    );
}

#[test]
fn a_group_written_as_an_array_is_not_split() {
    assert_same(
        &operation_with(r#""x-fern-sdk-group-name": ["a.b", "c"], "x-fern-sdk-method-name": "get","#),
        &operation_with(r#""x-forge-group-name": ["a.b", "c"], "x-forge-method-name": "get","#),
    );
}

/// A group path is names separated by dots when it is a string, as for the
/// native key, and the names of an array, which are not split, when it is not.
#[test]
fn a_group_is_split_on_dots_only_when_it_is_a_string() {
    for (written, names) in [
        (r#""a.b""#, vec!["a", "b"]),
        (r#"["a", "b"]"#, vec!["a", "b"]),
        (r#"["a.b"]"#, vec!["a.b"]),
        (r#"["a.b", "c"]"#, vec!["a.b", "c"]),
        (r#"["a", "b.c"]"#, vec!["a", "b.c"]),
    ] {
        let forge = parse_normalized(&operation_with(&format!(
            r#""x-fern-sdk-group-name": {written}, "x-fern-sdk-method-name": "get","#
        )));

        let leaf = forge.groups().find(|group| group.methods().count() > 0).unwrap();
        let want: Vec<forge::Name> = names.iter().map(|name| forge::Name::new(name)).collect();
        assert!(leaf.path().eq(want.iter()), "{written} is not the path {names:?}");
    }
}

#[test]
fn an_empty_group_string_is_no_group_and_so_no_method() {
    let forge = parse_normalized(&operation_with(
        r#""x-fern-sdk-group-name": "", "x-fern-sdk-method-name": "get","#,
    ));

    assert_eq!(forge.groups().count(), 0);
}

#[test]
fn a_group_that_is_neither_a_string_nor_an_array_is_an_error() {
    // The value is written as it was, under the native key, which `forge`
    // then says it does not accept.
    let err = parse_err(&normalized(&operation_with(
        r#""x-fern-sdk-group-name": 1, "x-fern-sdk-method-name": "get","#,
    )));

    assert!(err.contains("a string or an array of strings"), "{err}");
}

#[test]
fn ignore_makes_no_method() {
    assert_same(
        &operation_with(r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-ignore": true,"#),
        &operation_with(r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-skip": true,"#),
    );
    assert_eq!(
        parse_normalized(&operation_with(
            r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-ignore": true,"#
        ))
        .groups()
        .count(),
        0
    );
}

#[test]
fn ignore_set_to_false_keeps_the_method() {
    assert_same(
        &operation_with(r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-ignore": false,"#),
        &operation_with(r#""x-forge-group-name": ["a"], "x-forge-method-name": "get","#),
    );
}

#[test]
fn a_parameter_name_is_read_as_native() {
    let parameters = |key: &str| {
        operation_with(&format!(
            r#""x-forge-group-name": ["a"], "x-forge-method-name": "list",
              "parameters": [{{"name": "q", "in": "query", "{key}": "query", "schema": {{"type": "string"}}}}],"#
        ))
    };

    assert_same(
        &parameters("x-fern-parameter-name"),
        &parameters("x-forge-parameter-name"),
    );
    assert!(describe(&parse_normalized(&parameters("x-fern-parameter-name"))).contains("q:query="));
}

#[test]
fn a_property_name_is_read_as_native() {
    let schema = |key: &str| {
        responding_with(
            &format!(r#"{{"type": "object", "properties": {{"owner": {{"type": "string", "{key}": "person"}}}}}}"#),
            "{}",
        )
    };

    assert_same(&schema("x-fern-property-name"), &schema("x-forge-property-name"));
    assert!(describe(&parse_normalized(&schema("x-fern-property-name"))).contains("owner:person="));
}

#[test]
fn a_type_name_is_read_as_the_name_of_a_model() {
    let schemas = |key: &str| {
        responding_with(
            r##"{"$ref": "#/components/schemas/not_found"}"##,
            &format!(r#"{{"not_found": {{"type": "object", "{key}": "Missing"}}}}"#),
        )
    };

    assert_same(&schemas("x-fern-type-name"), &schemas("x-forge-model-name"));
    assert!(describe(&parse_normalized(&schemas("x-fern-type-name"))).contains("model missing ="));
}

#[test]
fn the_keys_of_an_alias_are_read_too() {
    let alias = |group: &str, method: &str, availability: &str, ignore: &str| {
        operation_with(&format!(
            r#""x-forge-aliases": [{{"{group}": {}, "{method}": "pause", "{availability}": "beta", "{ignore}": false}}],"#,
            if group.starts_with("x-fern") {
                r#""a.b""#
            } else {
                r#"["a", "b"]"#
            },
        ))
    };

    assert_same(
        &alias(
            "x-fern-sdk-group-name",
            "x-fern-sdk-method-name",
            "x-fern-availability",
            "x-fern-ignore",
        ),
        &alias(
            "x-forge-group-name",
            "x-forge-method-name",
            "x-forge-availability",
            "x-forge-skip",
        ),
    );
}

#[test]
fn a_key_that_is_read_is_not_an_extension_of_the_method() {
    let forge = parse_normalized(&operation_with(
        r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-ignore": false,
          "x-fern-availability": "beta", "x-a": 1,"#,
    ));
    let method = forge.groups().next().unwrap().methods().next().unwrap();

    assert_eq!(method.extensions().map(|(name, _)| name).collect::<Vec<_>>(), ["x-a"]);
    for key in [
        "x-fern-sdk-group-name",
        "x-fern-sdk-method-name",
        "x-fern-ignore",
        "x-fern-availability",
    ] {
        assert_eq!(method.extension(key), None, "{key}");
    }
}

#[test]
fn a_key_that_is_not_read_stays_an_extension_of_the_method() {
    let forge = parse_normalized(&operation_with(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-fern-audiences": ["internal"], "x-fern-pagination": {"offset": "o"},"#,
    ));
    let method = forge.groups().next().unwrap().methods().next().unwrap();

    assert_eq!(
        method.extensions().map(|(name, _)| name).collect::<Vec<_>>(),
        ["x-fern-audiences", "x-fern-pagination"]
    );
}

#[test]
fn an_extension_on_a_schema_that_is_not_read_has_no_effect() {
    let with = responding_with(
        r#"{"type": "string", "x-fern-enum": {"a": {"name": "A"}}, "x-fern-audiences": ["x"]}"#,
        "{}",
    );
    let without = responding_with(r#"{"type": "string"}"#, "{}");

    assert_same(&with, &without);
}

#[test]
fn an_error_names_the_native_key_the_rewrite_wrote() {
    let err = parse_err(&normalized(&operation_with(
        r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-availability": "ga","#,
    )));
    assert!(err.contains(r#"GET /a: x-forge-availability "ga" is not"#), "{err}");

    let err = parse_err(&normalized(&operation_with(
        r#""x-fern-sdk-group-name": "accounts..members", "x-fern-sdk-method-name": "get","#,
    )));
    assert!(
        err.contains("GET /a: x-forge-group-name has a group name with no letters or digits"),
        "{err}"
    );

    let err = parse_err(&normalized(&operation_with(
        r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "-","#,
    )));
    assert!(
        err.contains("GET /a: x-forge-method-name has no letters or digits"),
        "{err}"
    );
}

#[test]
fn an_error_in_an_alias_names_the_native_key_the_rewrite_wrote() {
    let err = parse_err(&normalized(&operation_with(
        r#""x-forge-aliases": [1, {"x-fern-sdk-group-name": ["a"], "x-fern-sdk-method-name": "pause", "x-fern-availability": "ga"}],"#,
    )));

    assert!(
        err.contains(r#"x-forge-aliases[1]: x-forge-availability "ga" is not"#),
        "{err}"
    );
}

#[test]
fn a_key_written_twice_is_left_as_it_is_with_a_warning_naming_it() {
    for (keys, key) in [
        (r#""x-fern-ignore": false, "x-fern-ignore": false,"#, "x-fern-ignore"),
        (
            r#""x-fern-sdk-method-name": "a", "x-fern-sdk-method-name": "b","#,
            "x-fern-sdk-method-name",
        ),
        (
            r#""x-fern-availability": "beta", "x-fern-availability": "beta","#,
            "x-fern-availability",
        ),
        (
            r#""x-fern-sdk-group-name": ["a"], "x-fern-sdk-group-name": ["b"],"#,
            "x-fern-sdk-group-name",
        ),
    ] {
        // `forge` reads it as an ordinary extension. There is no one thing it
        // says, so the rewrite leaves each, and does not stop.
        let document = operation_with(keys);
        parse(&document);

        let (text, left) = normalized_and_left(&document);

        assert_eq!(text, document, "{key}");
        assert_eq!(
            left,
            [(
                "/paths/~1a/get".to_owned(),
                key.to_owned(),
                "is written twice in this object"
            )]
        );
    }
}

#[test]
fn the_vendor_keys_of_a_kind_of_object_are_read_only_there() {
    // A name key on an operation, or a method key on a schema, says nothing.
    let forge = parse_normalized(&operation_with(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-fern-property-name": "p", "x-fern-type-name": "T",
          "x-fern-parameter-name": "q","#,
    ));
    let method = forge.groups().next().unwrap().methods().next().unwrap();
    assert_eq!(method.extensions().count(), 3);

    let schema = responding_with(
        r#"{"type": "object", "properties": {"a": {"type": "string", "x-fern-sdk-method-name": "m", "x-fern-ignore": true}}}"#,
        "{}",
    );
    assert!(
        describe(&parse_normalized(&schema)).contains("a:a="),
        "{}",
        describe(&parse_normalized(&schema))
    );
}
