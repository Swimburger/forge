//! The adapter for Fern's `x-fern-*` extensions.
//!
//! A group path written as one string is split on dots, as for
//! `x-forge-group-name`, which Fern itself does not do: Fern reads a string as a
//! single group. A document written for Fern uses the array, which means the
//! same in both.
//!
//! Its other extensions, such as `x-fern-enum` and `x-fern-audiences`, are
//! not read: they say things forge has no primitive for.

/// What every key of the adapter begins with. A key without it is turned
/// away before it is matched against the table.
const PREFIX: &str = "x-fern-";

/// The keys of an operation or an alias.
pub(super) mod definition {
    use super::PREFIX;
    use crate::metadata::{BoolFormat, DefinitionField, Hit};

    const METHOD_NAME: &str = "x-fern-sdk-method-name";
    const GROUP_NAME: &str = "x-fern-sdk-group-name";
    const IGNORE: &str = "x-fern-ignore";
    const AVAILABILITY: &str = "x-fern-availability";

    /// The names of the adapter's keys of an operation or an alias, by slot.
    pub(crate) const NAMES: [&str; 4] = [METHOD_NAME, GROUP_NAME, IGNORE, AVAILABILITY];

    pub(crate) fn read(key: &str) -> Option<Hit<DefinitionField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            METHOD_NAME => Hit::compat(DefinitionField::Method, 0),
            GROUP_NAME => Hit::compat(DefinitionField::Group, 1),
            IGNORE => Hit::compat(DefinitionField::Skip(BoolFormat::Bool), 2),
            AVAILABILITY => Hit::compat(DefinitionField::Availability, 3),
            _ => return None,
        })
    }
}

/// The keys of a Parameter Object.
pub(super) mod parameter {
    use super::PREFIX;
    use crate::metadata::{Hit, ParameterField};

    const NAME: &str = "x-fern-parameter-name";

    /// The names of the adapter's keys of a Parameter Object, by slot.
    pub(crate) const NAMES: [&str; 1] = [NAME];

    pub(crate) fn read(key: &str) -> Option<Hit<ParameterField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            NAME => Hit::compat(ParameterField::Name, 0),
            _ => return None,
        })
    }
}

/// The keys of a Schema Object.
pub(super) mod schema {
    use super::PREFIX;
    use crate::metadata::{Hit, SchemaField};

    const TYPE_NAME: &str = "x-fern-type-name";
    const PROPERTY_NAME: &str = "x-fern-property-name";

    /// The names of the adapter's keys of a Schema Object, by slot.
    pub(crate) const NAMES: [&str; 2] = [TYPE_NAME, PROPERTY_NAME];

    pub(crate) fn read(key: &str) -> Option<Hit<SchemaField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            TYPE_NAME => Hit::compat(SchemaField::ModelName, 0),
            PROPERTY_NAME => Hit::compat(SchemaField::PropertyName, 1),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names of the keys of a level, and whether the level reads a key.
    type Level<'a> = (&'a [&'a str], fn(&str) -> bool);
    use crate::metadata::{DefinitionField, Tier};

    #[test]
    fn each_key_is_read_as_itself_and_as_not_native() {
        for key in definition::NAMES {
            let hit = definition::read(key).unwrap_or_else(|| panic!("{key} is not a definition key"));
            assert_eq!(hit.tier, Tier::Compat, "{key}");
        }
        for key in parameter::NAMES {
            let hit = parameter::read(key).unwrap_or_else(|| panic!("{key} is not a parameter key"));
            assert_eq!(hit.tier, Tier::Compat, "{key}");
        }
        for key in schema::NAMES {
            let hit = schema::read(key).unwrap_or_else(|| panic!("{key} is not a schema key"));
            assert_eq!(hit.tier, Tier::Compat, "{key}");
        }
    }

    #[test]
    fn a_key_is_read_only_at_its_level() {
        let levels: [Level; 3] = [
            (&definition::NAMES, |key| definition::read(key).is_some()),
            (&parameter::NAMES, |key| parameter::read(key).is_some()),
            (&schema::NAMES, |key| schema::read(key).is_some()),
        ];
        for (own, _) in levels {
            for &key in own {
                for (names, reads) in levels {
                    assert_eq!(reads(key), names.contains(&key), "{key}");
                }
            }
        }
    }

    #[test]
    fn slots_are_unique_among_the_keys_of_an_object() {
        let mut slots: Vec<u8> = definition::NAMES
            .iter()
            .map(|key| definition::read(key).unwrap().slot)
            .collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), definition::NAMES.len());
        assert!(slots.iter().all(|&slot| slot >= 32), "slots of other keys start at 32");
    }

    #[test]
    fn the_group_is_read_as_the_native_group() {
        let hit = definition::read("x-fern-sdk-group-name").unwrap();
        assert!(matches!(hit.field, DefinitionField::Group));
    }

    #[test]
    fn unread_extensions_are_not_read() {
        for key in [
            "x-fern-enum",
            "x-fern-audiences",
            "x-fern-pagination",
            "x-fern-sdk-variables",
        ] {
            assert!(definition::read(key).is_none(), "{key}");
            assert!(parameter::read(key).is_none(), "{key}");
            assert!(schema::read(key).is_none(), "{key}");
        }
    }
}
