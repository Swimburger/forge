//! The adapter for Stainless's `x-stainless-*` extensions.
//!
//! | Key | Says | Format |
//! |---|---|---|
//! | `x-stainless-skip` | the property makes no field | a boolean, or an array of the targets to skip it for, which counts as `false` |
//!
//! Its other extensions, such as `x-stainless-terraform-configurability` and
//! `x-stainless-deprecation-message`, are not read: they say things forge has
//! no primitive for.

use crate::metadata::{BoolFormat, Hit, SchemaField};

/// What every key of the adapter begins with. A key without it is turned
/// away before it is matched against the table.
const PREFIX: &str = "x-stainless-";
const SKIP: &str = "x-stainless-skip";

/// The names of the adapter's keys of a Schema Object, by slot.
pub(super) const SCHEMA_NAMES: [&str; 1] = [SKIP];

pub(super) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    if !key.starts_with(PREFIX) {
        return None;
    }
    Some(match key {
        SKIP => Hit::compat(SchemaField::Ignore(BoolFormat::BoolOrTargets), 0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skip_is_read_on_a_schema_as_a_boolean_or_targets_and_not_native() {
        let hit = schema(SKIP).unwrap();
        assert!(!hit.is_native(), "{SKIP}");
        assert!(matches!(hit.field, SchemaField::Ignore(BoolFormat::BoolOrTargets)));
    }

    #[test]
    fn other_keys_are_not_read() {
        for key in [
            "x-stainless-terraform-configurability",
            "x-stainless-deprecation-message",
            "x-fern-ignore",
        ] {
            assert!(schema(key).is_none(), "{key}");
        }
    }
}
