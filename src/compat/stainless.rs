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

const SKIP: &str = "x-stainless-skip";

/// Slot 2 among the compatibility keys of a Schema Object: Fern's are the
/// first two.
pub(super) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    Some(match key {
        SKIP => Hit::compat(SchemaField::Ignore(BoolFormat::BoolOrTargets), SKIP, 2),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skip_is_read_on_a_schema_as_a_boolean_or_targets_and_not_native() {
        let hit = schema(SKIP).unwrap();
        assert_eq!((hit.key, hit.native), (SKIP, false));
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
