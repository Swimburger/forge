//! The vendor tables, and `normalize` on documents that are damaged. The second
//! is here, and not in `tests/normalize/`, as it is of what is private to
//! `normalize`, and names the keys of other tools, which only this module may.

use super::*;
use crate::metadata;
use crate::normalize::{Options, check, check_and_walk, normalize, walk_unchecked};

/// A document that has what is walked in it, with keys to rewrite at
/// each place, one written with an escape.
const DOCUMENT: &str = r#"{"openapi": "3.0.3", "info": {"title": "t", "version": "1"},
    "paths": {"/a/{id}": {"parameters": [{"name": "id", "in": "path", "x-fern-parameter-name": "i", "schema": {"type": "string"}}],
      "get": {"x-fern-sdk-group-name": ["a", "b"], "x-fern-sdk-method-name": "get", "x-fern-ignore": true,
        "x-forge-aliases": [{"x-fern-sdk-method-name": "old"}],
        "requestBody": {"content": {"application/json": {"schema": {"type": "object", "properties": {"p": {"type": "string", "x-stainless-skip": true}}}}}},
        "responses": {"200": {"description": "ok", "content": {"application/json": {"schema": {"type": "array", "items": {"type": "object", "x\u002dsensitive": true, "properties": {"q": {"x-sensitive": true}}}}}}}}}}},
    "components": {"schemas": {"s": {"allOf": [{"type": "string", "x-stainless-skip": ["go"]}], "$defs": {"d": {"x-sensitive": true}}}},
      "parameters": {"p": {"name": "n", "in": "query", "x-fern-parameter-name": "m", "schema": {"x-sensitive": true}}}}}"#;

const KEEP: Options = Options {
    leave_old_extensions: true,
};

/// Damaged copies of `document`: cut short, with a byte left out and with
/// a byte changed to something that is not what JSON is made of.
fn damaged(document: &str) -> Vec<Vec<u8>> {
    let junk = [b'"', b'{', b'}', b'[', b']', b',', b':', b'\\', 0xFF, b'x', b' ', 0];
    let bytes = document.as_bytes();
    let mut all: Vec<Vec<u8>> = (0..=bytes.len()).map(|end| bytes[..end].to_vec()).collect();
    for at in 0..bytes.len() {
        let mut without = bytes.to_vec();
        without.remove(at);
        all.push(without);
        for byte in junk {
            let mut changed = bytes.to_vec();
            changed[at] = byte;
            all.push(changed);
        }
    }
    all
}

/// The walk is only given a document that is JSON, as `normalize` stands,
/// when it is short, so it is given these as they are.
#[test]
fn the_walk_does_not_panic_or_loop_on_a_document_that_is_not_json() {
    let documents = damaged(DOCUMENT);
    assert!(documents.len() > 1000, "the damaged documents were made");
    for document in &documents {
        for options in [Options::default(), KEEP] {
            // What it makes of a document that is not JSON is not said, only
            // that it does, and that what it makes can be written.
            if let Ok((Some(edited), _)) = walk_unchecked(document, &options) {
                let _ = edited.to_vec();
            }
        }
    }
}

#[test]
fn a_document_is_rewritten_the_same_with_the_check_beside_the_walk_and_before_it() {
    for options in [Options::default(), KEEP] {
        let one = check_and_walk(DOCUMENT.as_bytes(), &options, false).unwrap();
        let both = check_and_walk(DOCUMENT.as_bytes(), &options, true).unwrap();
        assert_eq!(one.0.unwrap().to_vec(), both.0.unwrap().to_vec());
        assert_eq!(one.1.migrated, both.1.migrated);
    }
}

#[test]
fn a_document_that_is_not_json_is_an_error_beside_the_walk_as_it_is_before_it() {
    for document in damaged(DOCUMENT).into_iter().step_by(7) {
        let before = check_and_walk(&document, &Options::default(), false);
        let beside = check_and_walk(&document, &Options::default(), true);
        assert_eq!(
            before.is_err(),
            beside.is_err(),
            "{}",
            String::from_utf8_lossy(&document)
        );
        if let (Err(before), Err(beside)) = (before, beside) {
            assert_eq!(before.to_string(), beside.to_string());
        }
    }
}

#[test]
fn what_is_written_from_a_damaged_document_that_is_still_json_is_json() {
    let mut rewritten = 0;
    for document in damaged(DOCUMENT) {
        if check(&document).is_err() {
            continue;
        }
        for options in [Options::default(), KEEP] {
            if let Some(edited) = normalize(&document, &options).unwrap().json {
                check(&edited.to_vec()).unwrap();
                rewritten += 1;
            }
        }
    }
    assert!(rewritten > 0, "some of them were still JSON, with keys to rewrite");
}

/// The keys of each kind of object that the tables read, with what each says.
fn definitions() -> Vec<(&'static str, Key<DefinitionField>)> {
    fern::definition::NAMES
        .iter()
        .map(|&key| (key, definition(key).unwrap_or_else(|| panic!("{key} is not read"))))
        .collect()
}

fn parameters() -> Vec<(&'static str, Key<ParameterField>)> {
    fern::parameter::NAMES
        .iter()
        .map(|&key| (key, parameter(key).unwrap_or_else(|| panic!("{key} is not read"))))
        .collect()
}

fn schemas() -> Vec<(&'static str, Key<SchemaField>)> {
    [
        &fern::schema::NAMES[..],
        &stainless::schema::NAMES,
        &unprefixed::schema::NAMES,
    ]
    .concat()
    .into_iter()
    .map(|key| (key, schema(key).unwrap_or_else(|| panic!("{key} is not read"))))
    .collect()
}

#[test]
fn each_key_says_the_native_field_it_stands_for() {
    for (key, want) in [
        ("x-fern-sdk-method-name", DefinitionField::Method),
        ("x-fern-sdk-group-name", DefinitionField::Group),
        ("x-fern-ignore", DefinitionField::Skip),
        ("x-fern-availability", DefinitionField::Availability),
    ] {
        let read = definition(key).unwrap();
        assert!(
            std::mem::discriminant(&read.field) == std::mem::discriminant(&want),
            "{key}"
        );
    }
    assert!(matches!(
        parameter("x-fern-parameter-name").unwrap().field,
        ParameterField::Name
    ));
    for (key, want) in [
        ("x-fern-type-name", SchemaField::ModelName),
        ("x-fern-property-name", SchemaField::PropertyName),
        ("x-stainless-skip", SchemaField::Skip),
        ("x-sensitive", SchemaField::Sensitive),
    ] {
        let read = schema(key).unwrap();
        assert!(
            std::mem::discriminant(&read.field) == std::mem::discriminant(&want),
            "{key}"
        );
    }
}

#[test]
fn only_the_skip_of_a_schema_that_stainless_writes_may_be_a_list_of_targets() {
    for (key, read) in definitions() {
        assert!(!read.targets, "{key}");
    }
    for (key, read) in parameters() {
        assert!(!read.targets, "{key}");
    }
    for (key, read) in schemas() {
        assert_eq!(read.targets, key == "x-stainless-skip", "{key}");
    }
}

#[test]
fn a_key_is_read_only_at_its_level() {
    let all = [
        fern::definition::NAMES.to_vec(),
        fern::parameter::NAMES.to_vec(),
        [
            &fern::schema::NAMES[..],
            &stainless::schema::NAMES,
            &unprefixed::schema::NAMES,
        ]
        .concat(),
    ];
    for (level, keys) in all.iter().enumerate() {
        for &key in keys {
            let read = [
                definition(key).is_some(),
                parameter(key).is_some(),
                schema(key).is_some(),
            ];
            for (other, reads) in read.into_iter().enumerate() {
                assert_eq!(reads, other == level, "{key}");
            }
        }
    }
}

#[test]
fn what_a_key_says_has_a_native_key_that_reads_as_it() {
    for (key, read) in definitions() {
        let native = metadata::definition::key(read.field);
        let hit = metadata::definition::read(native).unwrap_or_else(|| panic!("{key} has no native key"));
        assert!(
            std::mem::discriminant(&hit.field) == std::mem::discriminant(&read.field),
            "{key}"
        );
    }
    for (key, read) in parameters() {
        assert!(
            metadata::parameter::read(metadata::parameter::key(read.field)).is_some(),
            "{key}"
        );
    }
    for (key, read) in schemas() {
        let native = metadata::schema::key(read.field);
        let hit = metadata::schema::read(native).unwrap_or_else(|| panic!("{key} has no native key"));
        assert!(
            std::mem::discriminant(&hit.field) == std::mem::discriminant(&read.field),
            "{key}"
        );
    }
}

#[test]
fn no_key_of_another_tool_is_a_native_key() {
    for (key, _) in definitions() {
        assert!(metadata::definition::read(key).is_none(), "{key}");
    }
    for (key, _) in parameters() {
        assert!(metadata::parameter::read(key).is_none(), "{key}");
    }
    for (key, _) in schemas() {
        assert!(metadata::schema::read(key).is_none(), "{key}");
    }
}

#[test]
fn the_other_extensions_of_these_tools_are_not_read() {
    for key in [
        "x-fern-enum",
        "x-fern-audiences",
        "x-fern-pagination",
        "x-fern-sdk-variables",
        "x-stainless-terraform-configurability",
        "x-stainless-deprecation-message",
        // The native keys, which are not for a rewrite.
        "x-forge-skip",
        "x-forge-method-name",
    ] {
        assert!(definition(key).is_none(), "{key}");
        assert!(parameter(key).is_none(), "{key}");
        assert!(schema(key).is_none(), "{key}");
    }
}
