//! The adapter for Fern's `x-fern-*` extensions.
//!
//! | Key | Says | Format |
//! |---|---|---|
//! | `x-fern-sdk-method-name` | the name of the method | a string |
//! | `x-fern-sdk-group-name` | the group path of the method | as `x-forge-group-name`: an array of group names, or one string of them separated by dots |
//! | `x-fern-ignore` | the definition makes no method | a boolean |
//! | `x-fern-availability` | the stage of the API | a string, one of the native spellings |
//! | `x-fern-parameter-name` | the name of a parameter's field | a string |
//! | `x-fern-property-name` | the name of a property's field | a string |
//! | `x-fern-type-name` | the name of a model | a string |
//!
//! A group path written as one string is split on dots, as for
//! `x-forge-group-name`, which Fern itself does not do: Fern reads a string as a
//! single group. A document written for Fern uses the array, which means the
//! same in both.
//!
//! Its other extensions, such as `x-fern-enum` and `x-fern-audiences`, are
//! not read: they say things forge has no primitive for.

use crate::metadata::{BoolFormat, DefinitionField, Hit, ParameterField, SchemaField};

/// What every key of the adapter begins with. A key without it is turned
/// away before it is matched against the table.
const PREFIX: &str = "x-fern-";
const METHOD_NAME: &str = "x-fern-sdk-method-name";
const GROUP_NAME: &str = "x-fern-sdk-group-name";
const IGNORE: &str = "x-fern-ignore";
const AVAILABILITY: &str = "x-fern-availability";
const PARAMETER_NAME: &str = "x-fern-parameter-name";
const PROPERTY_NAME: &str = "x-fern-property-name";
const TYPE_NAME: &str = "x-fern-type-name";

/// The names of the adapter's keys of an operation or an alias, by slot.
pub(super) const DEFINITION_NAMES: [&str; 4] = [METHOD_NAME, GROUP_NAME, IGNORE, AVAILABILITY];
/// The names of the adapter's keys of a Parameter Object, by slot.
pub(super) const PARAMETER_NAMES: [&str; 1] = [PARAMETER_NAME];
/// The names of the adapter's keys of a Schema Object, by slot.
pub(super) const SCHEMA_NAMES: [&str; 2] = [TYPE_NAME, PROPERTY_NAME];

pub(super) fn definition(key: &str) -> Option<Hit<DefinitionField>> {
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

pub(super) fn parameter(key: &str) -> Option<Hit<ParameterField>> {
    if !key.starts_with(PREFIX) {
        return None;
    }
    Some(match key {
        PARAMETER_NAME => Hit::compat(ParameterField::Name, 0),
        _ => return None,
    })
}

pub(super) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    if !key.starts_with(PREFIX) {
        return None;
    }
    Some(match key {
        TYPE_NAME => Hit::compat(SchemaField::ModelName, 0),
        PROPERTY_NAME => Hit::compat(SchemaField::PropertyName, 1),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Tier;

    #[test]
    fn each_key_is_read_as_itself_and_as_not_native() {
        for key in [METHOD_NAME, GROUP_NAME, IGNORE, AVAILABILITY] {
            let hit = definition(key).unwrap_or_else(|| panic!("{key} is not a definition key"));
            assert_eq!(hit.tier, Tier::Compat, "{key}");
        }
        let hit = parameter(PARAMETER_NAME).unwrap();
        assert_eq!(hit.tier, Tier::Compat, "{PARAMETER_NAME}");
        for key in [TYPE_NAME, PROPERTY_NAME] {
            let hit = schema(key).unwrap_or_else(|| panic!("{key} is not a schema key"));
            assert_eq!(hit.tier, Tier::Compat, "{key}");
        }
    }

    #[test]
    fn a_key_is_read_only_where_it_says_something() {
        assert!(definition(PARAMETER_NAME).is_none());
        assert!(definition(PROPERTY_NAME).is_none());
        assert!(parameter(METHOD_NAME).is_none());
        assert!(schema(IGNORE).is_none());
        assert!(schema(METHOD_NAME).is_none());
    }

    #[test]
    fn slots_are_unique_among_the_keys_of_an_object() {
        let mut slots: Vec<u8> = [METHOD_NAME, GROUP_NAME, IGNORE, AVAILABILITY]
            .iter()
            .map(|key| definition(key).unwrap().slot)
            .collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), 4);
        assert!(slots.iter().all(|&slot| slot >= 32), "slots of other keys start at 32");
    }

    #[test]
    fn the_group_is_read_as_the_native_group() {
        let hit = definition(GROUP_NAME).unwrap();
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
            assert!(definition(key).is_none(), "{key}");
            assert!(parameter(key).is_none(), "{key}");
            assert!(schema(key).is_none(), "{key}");
        }
    }
}
