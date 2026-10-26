//! Stainless's `x-stainless-skip`: a property that makes no field, written as
//! a boolean or as a list of targets.

use super::common::{assert_same, describe, operation_with, parse, parse_err, responding_with};

fn with_stainless_skip(value: &str) -> String {
    responding_with(
        &format!(
            r#"{{"type": "object", "properties": {{"id": {{"type": "string"}}, "old_id": {{"type": "string", "x-stainless-skip": {value}}}}}}}"#
        ),
        "{}",
    )
}

fn with_native_skip(value: &str) -> String {
    responding_with(
        &format!(
            r#"{{"type": "object", "properties": {{"id": {{"type": "string"}}, "old_id": {{"type": "string", "x-forge-skip": {value}}}}}}}"#
        ),
        "{}",
    )
}

#[test]
fn a_stainless_skip_is_read_as_a_native_one() {
    assert_same(&with_stainless_skip("true"), &with_native_skip("true"));
    assert!(!describe(&parse(&with_stainless_skip("true"))).contains("old_id"));
}

#[test]
fn a_property_skipped_as_false_keeps_its_field() {
    assert_same(&with_stainless_skip("false"), &with_native_skip("false"));
    assert!(describe(&parse(&with_stainless_skip("false"))).contains("old_id"));
}

#[test]
fn a_list_of_targets_is_not_a_skip() {
    assert_same(
        &with_stainless_skip(r#"["typescript", "go"]"#),
        &with_native_skip("false"),
    );
    assert!(describe(&parse(&with_stainless_skip(r#"["typescript"]"#))).contains("old_id"));
    assert!(describe(&parse(&with_stainless_skip("[]"))).contains("old_id"));
}

#[test]
fn a_skip_that_is_not_a_boolean_or_a_list_is_an_error() {
    let err = parse_err(&with_stainless_skip(r#""yes""#));

    assert!(err.contains("a boolean or an array of targets"), "{err}");
}

#[test]
fn a_list_of_targets_is_an_error_for_the_native_key() {
    let err = parse_err(&with_native_skip(r#"["typescript"]"#));

    assert!(err.contains("invalid type"), "{err}");
}

#[test]
fn a_skip_written_twice_is_an_error_naming_it() {
    let schema = responding_with(
        r#"{"type": "object", "properties": {"a": {"type": "string", "x-stainless-skip": false, "x-stainless-skip": false}}}"#,
        "{}",
    );

    assert!(parse_err(&schema).contains("duplicate field `x-stainless-skip`"));
}

#[test]
fn a_skip_on_an_operation_says_nothing_and_stays_an_extension() {
    let forge = parse(&operation_with(
        r#""x-forge-group-name": ["a"], "x-forge-method-name": "get", "x-stainless-skip": true,"#,
    ));
    let method = forge.groups().next().unwrap().methods().next().unwrap();

    assert_eq!(
        method.extensions().map(|(name, _)| name).collect::<Vec<_>>(),
        ["x-stainless-skip"]
    );
}

#[test]
fn the_other_stainless_keys_are_not_read() {
    let with = responding_with(
        r#"{"type": "object", "x-stainless-terraform-configurability": "computed",
            "properties": {"a": {"type": "string", "x-stainless-deprecation-message": "gone", "x-stainless-collection-type": "set"}}}"#,
        "{}",
    );
    let without = responding_with(r#"{"type": "object", "properties": {"a": {"type": "string"}}}"#, "{}");

    assert_same(&with, &without);
}
