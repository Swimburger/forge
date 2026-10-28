//! Stainless's `x-stainless-skip`: a property that makes no field, written as
//! a boolean or as a list of targets.

use super::common::{
    assert_same, describe, normalized, normalized_and_left, operation_with, parse_err, parse_normalized,
    responding_with,
};

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
    assert!(!describe(&parse_normalized(&with_stainless_skip("true"))).contains("old_id"));
}

#[test]
fn a_property_skipped_as_false_keeps_its_field() {
    assert_same(&with_stainless_skip("false"), &with_native_skip("false"));
    assert!(describe(&parse_normalized(&with_stainless_skip("false"))).contains("old_id"));
}

#[test]
fn a_list_of_targets_is_not_a_skip() {
    assert_same(
        &with_stainless_skip(r#"["typescript", "go"]"#),
        &with_native_skip("false"),
    );
    assert!(describe(&parse_normalized(&with_stainless_skip(r#"["typescript"]"#))).contains("old_id"));
    assert!(describe(&parse_normalized(&with_stainless_skip("[]"))).contains("old_id"));
}

#[test]
fn a_skip_that_is_not_a_boolean_or_a_list_is_an_error_for_the_native_key() {
    // Written as it was, which `forge` says is not a boolean.
    let err = parse_err(&normalized(&with_stainless_skip(r#""yes""#)));

    assert!(err.contains("invalid type") && err.contains("a boolean"), "{err}");
}

#[test]
fn a_list_of_targets_is_an_error_for_the_native_key() {
    let err = parse_err(&with_native_skip(r#"["typescript"]"#));

    assert!(err.contains("invalid type"), "{err}");
}

#[test]
fn a_skip_written_twice_is_left_as_it_is_with_a_warning_naming_it() {
    let schema = responding_with(
        r#"{"type": "object", "properties": {"a": {"type": "string", "x-stainless-skip": false, "x-stainless-skip": false}}}"#,
        "{}",
    );

    let (text, left) = normalized_and_left(&schema);

    assert_eq!(text, schema);
    assert_eq!(left.len(), 1);
    assert_eq!(
        (left[0].1.as_str(), left[0].2),
        ("x-stainless-skip", "is written twice in this object")
    );
}

#[test]
fn a_skip_on_an_operation_says_nothing_and_stays_an_extension() {
    let forge = parse_normalized(&operation_with(
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
