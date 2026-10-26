//! The adapter for Fern's `x-fern-*` extensions.
//!
//! | Key | Says | Format |
//! |---|---|---|
//! | `x-fern-sdk-method-name` | the name of the method | a string |
//! | `x-fern-sdk-group-name` | the group path of the method | an array of group names, or one string of them separated by dots |
//! | `x-fern-ignore` | the definition makes no method | a boolean |
//! | `x-fern-availability` | the stage of the API | a string, one of the native spellings |
//! | `x-fern-parameter-name` | the name of a parameter's field | a string |
//! | `x-fern-property-name` | the name of a property's field | a string |
//! | `x-fern-type-name` | the name of a model | a string |
//!
//! Its other extensions, such as `x-fern-enum` and `x-fern-audiences`, are
//! not read: they say things forge has no primitive for.

use crate::metadata::{BoolFormat, DefinitionField, GroupFormat, Hit, ParameterField, SchemaField};

const METHOD_NAME: &str = "x-fern-sdk-method-name";
const GROUP_NAME: &str = "x-fern-sdk-group-name";
const IGNORE: &str = "x-fern-ignore";
const AVAILABILITY: &str = "x-fern-availability";
const PARAMETER_NAME: &str = "x-fern-parameter-name";
const PROPERTY_NAME: &str = "x-fern-property-name";
const TYPE_NAME: &str = "x-fern-type-name";

pub(super) fn definition(key: &str) -> Option<Hit<DefinitionField>> {
    Some(match key {
        METHOD_NAME => Hit::compat(DefinitionField::Method, METHOD_NAME, 0),
        GROUP_NAME => Hit::compat(
            DefinitionField::Group(GroupFormat::StringOrArray { separator: '.' }),
            GROUP_NAME,
            1,
        ),
        IGNORE => Hit::compat(DefinitionField::Ignore(BoolFormat::Bool), IGNORE, 2),
        AVAILABILITY => Hit::compat(DefinitionField::Availability, AVAILABILITY, 3),
        _ => return None,
    })
}

pub(super) fn parameter(key: &str) -> Option<Hit<ParameterField>> {
    Some(match key {
        PARAMETER_NAME => Hit::compat(ParameterField::Name, PARAMETER_NAME, 0),
        _ => return None,
    })
}

/// Slots 0 and 1 of the compatibility keys of a Schema Object are Fern's;
/// the next adapter's start at 2.
pub(super) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    Some(match key {
        TYPE_NAME => Hit::compat(SchemaField::ModelName, TYPE_NAME, 0),
        PROPERTY_NAME => Hit::compat(SchemaField::PropertyName, PROPERTY_NAME, 1),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_is_read_as_itself_and_as_not_native() {
        for key in [METHOD_NAME, GROUP_NAME, IGNORE, AVAILABILITY] {
            let hit = definition(key).unwrap_or_else(|| panic!("{key} is not a definition key"));
            assert_eq!((hit.key, hit.native), (key, false));
        }
        let hit = parameter(PARAMETER_NAME).unwrap();
        assert_eq!((hit.key, hit.native), (PARAMETER_NAME, false));
        for key in [TYPE_NAME, PROPERTY_NAME] {
            let hit = schema(key).unwrap_or_else(|| panic!("{key} is not a schema key"));
            assert_eq!((hit.key, hit.native), (key, false));
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
    fn the_group_is_a_string_split_on_dots_or_an_array() {
        let hit = definition(GROUP_NAME).unwrap();
        assert!(matches!(
            hit.field,
            DefinitionField::Group(GroupFormat::StringOrArray { separator: '.' })
        ));
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
