//! The adapter for Stainless's `x-stainless-*` extensions.
//!
//! `x-stainless-skip` may be a list of the targets to skip a property for,
//! which counts as `false`.
//!
//! Its other extensions, such as `x-stainless-terraform-configurability` and
//! `x-stainless-deprecation-message`, are not read: they say things forge has
//! no primitive for.

/// What every key of the adapter begins with. A key without it is turned
/// away before it is matched against the table.
const PREFIX: &str = "x-stainless-";

/// The keys of a Schema Object.
pub(super) mod schema {
    use super::PREFIX;
    use crate::metadata::{BoolFormat, Hit, SchemaField};

    const SKIP: &str = "x-stainless-skip";

    /// The names of the adapter's keys of a Schema Object, by slot.
    pub(crate) const NAMES: [&str; 1] = [SKIP];

    pub(crate) fn read(key: &str) -> Option<Hit<SchemaField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            SKIP => Hit::compat(SchemaField::Skip(BoolFormat::BoolOrTargets), 0),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{BoolFormat, SchemaField, Tier};

    #[test]
    fn the_skip_is_read_on_a_schema_as_a_boolean_or_targets_and_not_native() {
        let key = "x-stainless-skip";
        let hit = schema::read(key).unwrap();
        assert_eq!(hit.tier, Tier::Compat, "{key}");
        assert!(matches!(hit.field, SchemaField::Skip(BoolFormat::BoolOrTargets)));
    }

    #[test]
    fn other_keys_are_not_read() {
        for key in [
            "x-stainless-terraform-configurability",
            "x-stainless-deprecation-message",
            "x-fern-ignore",
        ] {
            assert!(schema::read(key).is_none(), "{key}");
        }
    }
}
