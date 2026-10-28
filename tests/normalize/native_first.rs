//! A native key wins: when a document has both a native key and the key of
//! another tool for the same thing, `forge normalize` keeps the native one,
//! whatever order they are written in and whatever the other value is, so long
//! as it is a value at all.

use super::common::{assert_same, describe, normalized, operation_with, parse_err, parse_normalized, responding_with};

/// Asserts that every ordering of `native` and `vendor`, which are members of
/// an operation, builds what `expected` alone does.
#[track_caller]
fn native_wins(native: &str, vendor: &str, expected: &str) {
    for keys in [format!("{native} {vendor}"), format!("{vendor} {native}")] {
        assert_same(&operation_with(&keys), &operation_with(expected));
    }
}

#[test]
fn the_native_method_wins() {
    native_wins(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "native","#,
        r#""x-fern-sdk-method-name": "vendor","#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "native","#,
    );
    assert!(
        describe(&parse_normalized(&operation_with(
            r#""x-forge-group-name": ["a"], "x-forge-method-name": "native", "x-fern-sdk-method-name": "vendor","#
        )))
        .contains("method native")
    );
}

#[test]
fn the_native_group_wins() {
    native_wins(
        r#""x-forge-method-name": "get", "x-forge-group-name": ["native"],"#,
        r#""x-fern-sdk-group-name": "vendor.group","#,
        r#""x-forge-method-name": "get", "x-forge-group-name": ["native"],"#,
    );
}

#[test]
fn the_native_availability_wins_and_an_invalid_vendor_one_is_not_checked() {
    native_wins(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
        r#""x-fern-availability": "ga","#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
    );
}

#[test]
fn a_native_skip_of_false_keeps_the_method_a_vendor_one_would_drop() {
    native_wins(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-skip": false,"#,
        r#""x-fern-ignore": true,"#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get","#,
    );
}

#[test]
fn a_native_skip_of_true_drops_the_method_a_vendor_one_would_keep() {
    native_wins(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-skip": true,"#,
        r#""x-fern-ignore": false,"#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-skip": true,"#,
    );
}

#[test]
fn the_native_parameter_name_wins() {
    let parameter = |keys: &str| {
        operation_with(&format!(
            r#""x-forge-group-name": ["a"], "x-forge-method-name": "list",
              "parameters": [{{"name": "q", "in": "query", {keys} "schema": {{"type": "string"}}}}],"#
        ))
    };
    let native = parameter(r#""x-forge-parameter-name": "native","#);

    for keys in [
        r#""x-forge-parameter-name": "native", "x-fern-parameter-name": "vendor","#,
        r#""x-fern-parameter-name": "vendor", "x-forge-parameter-name": "native","#,
    ] {
        assert_same(&parameter(keys), &native);
    }
    assert!(describe(&parse_normalized(&native)).contains("q:native="));
}

#[test]
fn the_native_property_and_model_names_win() {
    let schema = |keys: &str| {
        responding_with(
            &format!(r#"{{"type": "object", "properties": {{"owner": {{"type": "string", {keys}}}}}}}"#),
            "{}",
        )
    };
    let native = schema(r#""x-forge-property-name": "native""#);

    for keys in [
        r#""x-forge-property-name": "native", "x-fern-property-name": "vendor""#,
        r#""x-fern-property-name": "vendor", "x-forge-property-name": "native""#,
    ] {
        assert_same(&schema(keys), &native);
    }

    let model = |keys: &str| {
        responding_with(
            r##"{"$ref": "#/components/schemas/m"}"##,
            &format!(r#"{{"m": {{"type": "object", {keys}}}}}"#),
        )
    };
    for keys in [
        r#""x-forge-model-name": "Native", "x-fern-type-name": "Vendor""#,
        r#""x-fern-type-name": "Vendor", "x-forge-model-name": "Native""#,
    ] {
        assert_same(&model(keys), &model(r#""x-forge-model-name": "Native""#));
    }
}

#[test]
fn the_native_skip_of_a_property_wins_over_a_stainless_one() {
    let property = |keys: &str| {
        responding_with(
            &format!(r#"{{"type": "object", "properties": {{"a": {{"type": "string", {keys}}}}}}}"#),
            "{}",
        )
    };
    for keys in [
        r#""x-forge-skip": false, "x-stainless-skip": true"#,
        r#""x-stainless-skip": true, "x-forge-skip": false"#,
    ] {
        assert!(describe(&parse_normalized(&property(keys))).contains("a:a="), "{keys}");
    }
    for keys in [
        r#""x-forge-skip": true, "x-stainless-skip": false"#,
        r#""x-stainless-skip": false, "x-forge-skip": true"#,
    ] {
        assert!(!describe(&parse_normalized(&property(keys))).contains("a:a="), "{keys}");
    }
}

#[test]
fn both_keys_with_the_same_value_are_the_same_as_one() {
    assert_same(
        &operation_with(
            r#""x-forge-group-name": ["a", "b"], "x-fern-sdk-group-name": "a.b",
              "x-forge-method-name": "get", "x-fern-sdk-method-name": "get",
              "x-forge-availability": "beta", "x-fern-availability": "beta","#,
        ),
        &operation_with(
            r#""x-forge-group-name": ["a", "b"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
        ),
    );
}

#[test]
fn each_primitive_is_resolved_on_its_own() {
    // The group is native and the method and availability are not.
    assert_same(
        &operation_with(
            r#""x-forge-group-name": ["a"], "x-fern-sdk-method-name": "get", "x-fern-availability": "beta","#,
        ),
        &operation_with(
            r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
        ),
    );
}

#[test]
fn the_value_of_a_key_beside_a_native_key_is_not_checked_whatever_it_is() {
    // It is dropped, as the native key says it, so it does not matter that
    // it is not even a value `forge` would take.
    for keys in [
        r#""x-forge-availability": "beta", "x-fern-availability": 1,"#,
        r#""x-fern-availability": 1, "x-forge-availability": "beta","#,
    ] {
        let document = operation_with(&format!(
            r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", {keys}"#
        ));

        assert_same(
            &document,
            &operation_with(
                r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
            ),
        );
    }
}

#[test]
fn a_native_key_written_twice_is_an_error_naming_it() {
    let err = parse_err(&operation_with(r#""x-forge-skip": false, "x-forge-skip": false,"#));

    assert!(err.contains("duplicate field `x-forge-skip`"), "{err}");
}

#[test]
fn the_native_group_written_as_a_string_wins() {
    native_wins(
        r#""x-forge-method-name": "get", "x-forge-group-name": "native.group","#,
        r#""x-fern-sdk-group-name": ["vendor"],"#,
        r#""x-forge-method-name": "get", "x-forge-group-name": ["native", "group"],"#,
    );
}

#[test]
fn an_error_names_the_native_key_when_the_native_key_gave_the_value() {
    let err = parse_err(&operation_with(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "ga","#,
    ));

    assert!(err.contains(r#"GET /a: x-forge-availability "ga" is not"#), "{err}");
}

#[test]
fn only_the_winning_group_and_method_name_are_checked() {
    // Each vendor value is an error on its own, so the document parsing at
    // all shows it was not checked.
    native_wins(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get","#,
        r#""x-fern-sdk-group-name": "a..b","#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get","#,
    );
    native_wins(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get","#,
        r#""x-fern-sdk-method-name": "-","#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get","#,
    );
    // Alone, each is written under the native key, which `forge` then checks.
    assert!(
        parse_err(&normalized(&operation_with(
            r#""x-fern-sdk-group-name": "a..b", "x-forge-method-name": "get","#
        )))
        .contains("x-forge-group-name")
    );
    assert!(
        parse_err(&normalized(&operation_with(
            r#""x-forge-group-name": ["a"], "x-fern-sdk-method-name": "-","#
        )))
        .contains("x-forge-method-name")
    );
}

/// Asserts that every ordering of `native` and `vendor`, which are members of
/// an alias, builds what an alias of `expected` alone does, and that it has a
/// method named `method`.
#[track_caller]
fn native_wins_in_an_alias(native: &str, vendor: &str, expected: &str, method: &str) {
    let alias = |keys: &str| operation_with(&format!(r#""x-forge-aliases": [{{{keys}}}],"#));
    for keys in [format!("{native}, {vendor}"), format!("{vendor}, {native}")] {
        assert_same(&alias(&keys), &alias(expected));
    }
    let described = describe(&parse_normalized(&alias(expected)));
    assert!(described.contains(&format!("method {method} ")), "{described}");
}

#[test]
fn the_native_keys_of_an_alias_win() {
    native_wins_in_an_alias(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "native""#,
        r#""x-fern-sdk-method-name": "vendor""#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "native""#,
        "native",
    );
    native_wins_in_an_alias(
        r#""x-forge-method-name": "get", "x-forge-group-name": ["native"]"#,
        r#""x-fern-sdk-group-name": "vendor.group""#,
        r#""x-forge-method-name": "get", "x-forge-group-name": ["native"]"#,
        "get",
    );
    native_wins_in_an_alias(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta""#,
        r#""x-fern-availability": "ga""#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta""#,
        "get",
    );
    native_wins_in_an_alias(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-forge-skip": false"#,
        r#""x-fern-ignore": true"#,
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get""#,
        "get",
    );
}

#[test]
fn a_native_skip_in_an_alias_drops_the_method_a_vendor_one_would_keep() {
    let alias = |keys: &str| {
        operation_with(&format!(
            r#""x-forge-aliases": [{{"x-forge-group-name": ["a"], "x-forge-method-name": "get", {keys}}}],"#
        ))
    };
    for keys in [
        r#""x-forge-skip": true, "x-fern-ignore": false"#,
        r#""x-fern-ignore": false, "x-forge-skip": true"#,
    ] {
        assert_eq!(parse_normalized(&alias(keys)).groups().count(), 0, "{keys}");
    }
}

/// `x-sensitive` is a key of another system as far as `forge` goes: it is the
/// rewrite that says what it means.
#[test]
fn x_forge_sensitive_wins_over_x_sensitive_in_either_order() {
    let property = |keys: &str| {
        responding_with(
            &format!(r#"{{"type": "object", "properties": {{"password": {{"type": "string", {keys}}}}}}}"#),
            "{}",
        )
    };
    for (keys, sensitive) in [
        (r#""x-forge-sensitive": false, "x-sensitive": true"#, false),
        (r#""x-sensitive": true, "x-forge-sensitive": false"#, false),
        (r#""x-forge-sensitive": true, "x-sensitive": false"#, true),
        (r#""x-sensitive": false, "x-forge-sensitive": true"#, true),
    ] {
        let described = describe(&parse_normalized(&property(keys)));

        assert_eq!(
            described.contains("password:password=String!"),
            sensitive,
            "{keys}\n{described}"
        );
        assert_same(
            &property(keys),
            &property(&format!(r#""x-forge-sensitive": {sensitive}"#)),
        );
    }
}

#[test]
fn x_sensitive_alone_makes_the_field_sensitive_through_the_rewrite_and_not_without_it() {
    let document = responding_with(
        r#"{"type": "object", "properties": {"password": {"type": "string", "x-sensitive": true}}}"#,
        "{}",
    );

    assert!(!describe(&super::common::parse(&document)).contains('!'));
    assert!(describe(&parse_normalized(&document)).contains("password:password=String!"));
}
