//! A native key is applied, and the compatibility layer is the fallback: when
//! a document has both, the compat value is ignored, whatever order they are
//! written in and whatever the compat value is, so long as it is a value at
//! all.

use super::common::{assert_same, describe, operation_with, parse, parse_err, responding_with};

/// Asserts that every ordering of `native` and `compat`, which are members of
/// an operation, builds what `expected` alone does.
#[track_caller]
fn native_wins(native: &str, compat: &str, expected: &str) {
    for keys in [format!("{native} {compat}"), format!("{compat} {native}")] {
        assert_same(&operation_with(&keys), &operation_with(expected));
    }
}

#[test]
fn the_native_method_wins() {
    native_wins(
        r#""x-forge-group": ["a"], "x-forge-method-name": "native","#,
        r#""x-fern-sdk-method-name": "vendor","#,
        r#""x-forge-group": ["a"], "x-forge-method-name": "native","#,
    );
    assert!(
        describe(&parse(&operation_with(
            r#""x-forge-group": ["a"], "x-forge-method-name": "native", "x-fern-sdk-method-name": "vendor","#
        )))
        .contains("method native")
    );
}

#[test]
fn the_native_group_wins() {
    native_wins(
        r#""x-forge-method-name": "get", "x-forge-group": ["native"],"#,
        r#""x-fern-sdk-group-name": "vendor.group","#,
        r#""x-forge-method-name": "get", "x-forge-group": ["native"],"#,
    );
}

#[test]
fn the_native_availability_wins_and_an_invalid_compat_one_is_not_checked() {
    native_wins(
        r#""x-forge-group": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
        r#""x-fern-availability": "ga","#,
        r#""x-forge-group": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
    );
}

#[test]
fn a_native_ignore_of_false_keeps_the_method_a_compat_one_would_drop() {
    native_wins(
        r#""x-forge-group": ["a"], "x-forge-method-name": "get", "x-forge-ignore": false,"#,
        r#""x-fern-ignore": true,"#,
        r#""x-forge-group": ["a"], "x-forge-method-name": "get","#,
    );
}

#[test]
fn a_native_ignore_of_true_drops_the_method_a_compat_one_would_keep() {
    native_wins(
        r#""x-forge-group": ["a"], "x-forge-method-name": "get", "x-forge-ignore": true,"#,
        r#""x-fern-ignore": false,"#,
        r#""x-forge-group": ["a"], "x-forge-method-name": "get", "x-forge-ignore": true,"#,
    );
}

#[test]
fn the_native_parameter_name_wins() {
    let parameter = |keys: &str| {
        operation_with(&format!(
            r#""x-forge-group": ["a"], "x-forge-method-name": "list",
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
    assert!(describe(&parse(&native)).contains("q:native="));
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
fn the_native_ignore_of_a_property_wins_over_a_skip() {
    let property = |keys: &str| {
        responding_with(
            &format!(r#"{{"type": "object", "properties": {{"a": {{"type": "string", {keys}}}}}}}"#),
            "{}",
        )
    };
    for keys in [
        r#""x-forge-ignore": false, "x-stainless-skip": true"#,
        r#""x-stainless-skip": true, "x-forge-ignore": false"#,
    ] {
        assert!(describe(&parse(&property(keys))).contains("a:a="), "{keys}");
    }
    for keys in [
        r#""x-forge-ignore": true, "x-stainless-skip": false"#,
        r#""x-stainless-skip": false, "x-forge-ignore": true"#,
    ] {
        assert!(!describe(&parse(&property(keys))).contains("a:a="), "{keys}");
    }
}

#[test]
fn both_keys_with_the_same_value_are_the_same_as_one() {
    assert_same(
        &operation_with(
            r#""x-forge-group": ["a", "b"], "x-fern-sdk-group-name": "a.b",
              "x-forge-method-name": "get", "x-fern-sdk-method-name": "get",
              "x-forge-availability": "beta", "x-fern-availability": "beta","#,
        ),
        &operation_with(
            r#""x-forge-group": ["a", "b"], "x-forge-method-name": "get", "x-forge-availability": "beta","#,
        ),
    );
}

#[test]
fn each_primitive_is_resolved_on_its_own() {
    // The group is native and the method and availability are not.
    assert_same(
        &operation_with(r#""x-forge-group": ["a"], "x-fern-sdk-method-name": "get", "x-fern-availability": "beta","#),
        &operation_with(r#""x-forge-group": ["a"], "x-forge-method-name": "get", "x-forge-availability": "beta","#),
    );
}

#[test]
fn a_compat_value_of_the_wrong_type_is_an_error_even_when_a_native_key_wins() {
    for keys in [
        r#""x-forge-availability": "beta", "x-fern-availability": 1,"#,
        r#""x-fern-availability": 1, "x-forge-availability": "beta","#,
    ] {
        let err = parse_err(&operation_with(&format!(
            r#""x-forge-group": ["a"], "x-forge-method-name": "get", {keys}"#
        )));

        assert!(err.contains("invalid type"), "{keys}: {err}");
    }
}

#[test]
fn a_native_key_written_twice_is_an_error_naming_it() {
    let err = parse_err(&operation_with(r#""x-forge-ignore": false, "x-forge-ignore": false,"#));

    assert!(err.contains("duplicate field `x-forge-ignore`"), "{err}");
}

#[test]
fn a_native_group_written_as_a_string_is_an_error() {
    let err = parse_err(&operation_with(
        r#""x-forge-group": "a.b", "x-forge-method-name": "get","#,
    ));

    assert!(err.contains("an array of strings"), "{err}");
}

#[test]
fn an_error_names_the_native_key_when_the_native_key_gave_the_value() {
    let err = parse_err(&operation_with(
        r#""x-forge-group": ["a"], "x-forge-method-name": "get", "x-forge-availability": "ga","#,
    ));

    assert!(err.contains(r#"GET /a: x-forge-availability "ga" is not"#), "{err}");
}
