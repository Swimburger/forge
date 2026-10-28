//! The native extensions: the `x-forge-*` keys a document uses to say what a
//! `Forge` is built from, and what each of them means.
//!
//! This is the whole vocabulary, for each kind of object that has any:
//! [`definition`] for an Operation Object or an alias in its
//! `x-forge-aliases`, [`parameter`], [`schema`] and [`document`]. Each reads
//! a key as the document writes it, and says what it is as a [`Hit`], or
//! `None` if the key is not one of them.
//!
//! Nothing here reads a document or allocates: the keys come in as `&str`s,
//! and what goes out is small values. Reading the values, and building a
//! `Forge` from them, is the work of `link`. It reads these keys and no
//! others: the extensions other tools write are ordinary extensions to it,
//! and `forge normalize` is what rewrites them into these.

use crate::Availability;

/// The `x-forge-aliases` key of an Operation Object. It is not in
/// [`definition`], since an alias cannot have one.
pub(crate) const ALIASES: &str = "x-forge-aliases";

/// What every native key begins with. A key without it is turned away before
/// it is matched against the table: most extensions in a document are no key
/// of any table, and this is the cheap way to say so.
const PREFIX: &str = "x-forge-";

/// A key that says something about the object it is on.
///
/// It is kept to a few bytes, since one is read for every key of every
/// object: the key's name, which only an error needs, is found from its
/// `slot`, in the table of names of its kind of object.
#[derive(Clone, Copy)]
pub(crate) struct Hit<F> {
    /// What the key says.
    pub(crate) field: F,
    /// The bit of the key in a set of the keys read from an object so far,
    /// which is how a key written twice is found, and the index of its name
    /// in the table of names of its kind of object.
    pub(crate) slot: u8,
}

impl<F> Hit<F> {
    /// Returns a key, in slot `slot`, which is under 64.
    const fn new(field: F, slot: u8) -> Hit<F> {
        Hit { field, slot }
    }
}

/// What a key says on an operation, or on an alias of one.
#[derive(Clone, Copy)]
pub(crate) enum DefinitionField {
    /// The name of the method.
    Method,
    /// The group path of the method: an array of group names, top level
    /// first, or one string of them separated by dots. The empty string is
    /// no group.
    Group,
    /// Whether the definition makes no method.
    Skip,
    /// The stage of the API the method is in.
    Availability,
    /// Whether the method is left out of listings.
    Hidden,
    /// The sentence to show before calling the method.
    Confirmation,
}

/// What a key says on a Parameter Object.
#[derive(Clone, Copy)]
pub(crate) enum ParameterField {
    /// The name of the parameter's field.
    Name,
}

/// What a key says on a Schema Object.
#[derive(Clone, Copy)]
pub(crate) enum SchemaField {
    /// The name of the model, for a schema in `components.schemas`.
    ModelName,
    /// The name of a property's field.
    PropertyName,
    /// Whether the property makes no field.
    Skip,
    /// Whether the schema's value is a secret.
    Sensitive,
}

/// What a key says at the root of the document.
#[derive(Clone, Copy)]
pub(crate) enum DocumentField {
    /// The descriptions of the groups.
    Commands,
}

/// Returns whether `key` is the `x-forge-aliases` of an operation.
pub(crate) fn is_aliases(key: &str) -> bool {
    key == ALIASES
}

/// The native keys of an operation, or of an alias in its `x-forge-aliases`.
pub(crate) mod definition {
    use super::{DefinitionField, Hit, PREFIX};

    const METHOD_NAME: &str = "x-forge-method-name";
    const GROUP_NAME: &str = "x-forge-group-name";
    const SKIP: &str = "x-forge-skip";
    const AVAILABILITY: &str = "x-forge-availability";
    const HIDDEN: &str = "x-forge-hidden";
    const CONFIRMATION: &str = "x-forge-require-confirmation";

    /// The names of the native keys of an operation or an alias, by slot.
    pub(crate) const NAMES: [&str; 6] = [METHOD_NAME, GROUP_NAME, SKIP, AVAILABILITY, HIDDEN, CONFIRMATION];

    /// Returns the native key that says what `field` says.
    pub(crate) fn key(field: DefinitionField) -> &'static str {
        match field {
            DefinitionField::Method => METHOD_NAME,
            DefinitionField::Group => GROUP_NAME,
            DefinitionField::Skip => SKIP,
            DefinitionField::Availability => AVAILABILITY,
            DefinitionField::Hidden => HIDDEN,
            DefinitionField::Confirmation => CONFIRMATION,
        }
    }

    /// Returns what `key` says on an operation or an alias, if it is a native
    /// key of one.
    pub(crate) fn read(key: &str) -> Option<Hit<DefinitionField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            METHOD_NAME => Hit::new(DefinitionField::Method, 0),
            GROUP_NAME => Hit::new(DefinitionField::Group, 1),
            SKIP => Hit::new(DefinitionField::Skip, 2),
            AVAILABILITY => Hit::new(DefinitionField::Availability, 3),
            HIDDEN => Hit::new(DefinitionField::Hidden, 4),
            CONFIRMATION => Hit::new(DefinitionField::Confirmation, 5),
            _ => return None,
        })
    }
}

/// The native keys of a Parameter Object.
pub(crate) mod parameter {
    use super::{Hit, PREFIX, ParameterField};

    const NAME: &str = "x-forge-parameter-name";

    /// The names of the native keys of a Parameter Object, by slot.
    pub(crate) const NAMES: [&str; 1] = [NAME];

    /// Returns the native key that says what `field` says.
    pub(crate) fn key(field: ParameterField) -> &'static str {
        match field {
            ParameterField::Name => NAME,
        }
    }

    /// Returns what `key` says on a Parameter Object, if it is a native key of
    /// one.
    pub(crate) fn read(key: &str) -> Option<Hit<ParameterField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            NAME => Hit::new(ParameterField::Name, 0),
            _ => return None,
        })
    }
}

/// The native keys of a Schema Object.
pub(crate) mod schema {
    use super::{Hit, PREFIX, SchemaField};

    const MODEL_NAME: &str = "x-forge-model-name";
    const PROPERTY_NAME: &str = "x-forge-property-name";
    const SKIP: &str = "x-forge-skip";
    const SENSITIVE: &str = "x-forge-sensitive";

    /// The names of the native keys of a Schema Object, by slot.
    pub(crate) const NAMES: [&str; 4] = [MODEL_NAME, PROPERTY_NAME, SKIP, SENSITIVE];

    /// Returns the native key that says what `field` says.
    pub(crate) fn key(field: SchemaField) -> &'static str {
        match field {
            SchemaField::ModelName => MODEL_NAME,
            SchemaField::PropertyName => PROPERTY_NAME,
            SchemaField::Skip => SKIP,
            SchemaField::Sensitive => SENSITIVE,
        }
    }

    /// Returns what `key` says on a Schema Object, if it is a native key of one.
    pub(crate) fn read(key: &str) -> Option<Hit<SchemaField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            MODEL_NAME => Hit::new(SchemaField::ModelName, 0),
            PROPERTY_NAME => Hit::new(SchemaField::PropertyName, 1),
            SKIP => Hit::new(SchemaField::Skip, 2),
            SENSITIVE => Hit::new(SchemaField::Sensitive, 3),
            _ => return None,
        })
    }
}

/// The native keys at the root of the document.
pub(crate) mod document {
    use super::{DocumentField, Hit, PREFIX};

    const COMMANDS: &str = "x-forge-commands";

    /// The names of the native keys of the document, by slot.
    pub(crate) const NAMES: [&str; 1] = [COMMANDS];

    /// Returns what `key` says at the root of the document, if it is a native
    /// key of it.
    pub(crate) fn read(key: &str) -> Option<Hit<DocumentField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            COMMANDS => Hit::new(DocumentField::Commands, 0),
            _ => return None,
        })
    }
}

/// Returns the availability `value` is, such as `beta`, or `None` if it is
/// none of them. Case matters.
pub(crate) fn availability(value: &str) -> Option<Availability> {
    match value {
        "alpha" => Some(Availability::Alpha),
        "beta" => Some(Availability::Beta),
        "preview" => Some(Availability::Preview),
        "generally-available" => Some(Availability::GenerallyAvailable),
        "deprecated" => Some(Availability::Deprecated),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names of the keys of a level, and whether the level reads a key.
    type Level<'a> = (&'a [&'a str], fn(&str) -> bool);

    /// Every native key of each kind of object, in slot order.
    fn native_keys() -> [&'static [&'static str]; 4] {
        [
            &[
                "x-forge-method-name",
                "x-forge-group-name",
                "x-forge-skip",
                "x-forge-availability",
                "x-forge-hidden",
                "x-forge-require-confirmation",
            ],
            &["x-forge-parameter-name"],
            &[
                "x-forge-model-name",
                "x-forge-property-name",
                "x-forge-skip",
                "x-forge-sensitive",
            ],
            &["x-forge-commands"],
        ]
    }

    /// Asserts that each of `keys` is read by `read` in the slot that names it
    /// in `names`.
    #[track_caller]
    fn read_as_itself<F>(keys: &[&str], names: &[&'static str], read: fn(&str) -> Option<Hit<F>>) {
        for &key in keys {
            let hit = read(key).unwrap_or_else(|| panic!("{key} is not read"));
            assert_eq!(names.get(usize::from(hit.slot)), Some(&key));
        }
        assert_eq!(keys.len(), names.len(), "every name is a key that is read");
    }

    #[test]
    fn each_native_key_is_read_as_itself() {
        let [definitions, parameters, schemas, documents] = native_keys();
        read_as_itself(definitions, &definition::NAMES, definition::read);
        read_as_itself(parameters, &parameter::NAMES, parameter::read);
        read_as_itself(schemas, &schema::NAMES, schema::read);
        read_as_itself(documents, &document::NAMES, document::read);
    }

    #[test]
    fn each_field_is_said_by_the_native_key_that_reads_as_it() {
        use std::mem::discriminant as same;

        for field in [
            DefinitionField::Method,
            DefinitionField::Group,
            DefinitionField::Skip,
            DefinitionField::Availability,
            DefinitionField::Hidden,
            DefinitionField::Confirmation,
        ] {
            let hit = definition::read(definition::key(field)).unwrap();
            assert!(same(&hit.field) == same(&field));
        }
        let hit = parameter::read(parameter::key(ParameterField::Name)).unwrap();
        assert!(matches!(hit.field, ParameterField::Name));
        for field in [
            SchemaField::ModelName,
            SchemaField::PropertyName,
            SchemaField::Skip,
            SchemaField::Sensitive,
        ] {
            let hit = schema::read(schema::key(field)).unwrap();
            assert!(same(&hit.field) == same(&field));
        }
    }

    #[test]
    fn a_native_key_is_read_only_at_its_level() {
        let levels: [Level; 4] = [
            (&definition::NAMES, |key| definition::read(key).is_some()),
            (&parameter::NAMES, |key| parameter::read(key).is_some()),
            (&schema::NAMES, |key| schema::read(key).is_some()),
            (&document::NAMES, |key| document::read(key).is_some()),
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
    fn slots_are_unique_among_the_keys_of_an_object_and_fit_in_a_set() {
        let [definitions, _, schemas, _] = native_keys();
        let slots = |keys: &[&str], hit: &dyn Fn(&str) -> u8| {
            let mut slots: Vec<u8> = keys.iter().map(|key| hit(key)).collect();
            slots.sort_unstable();
            let len = slots.len();
            slots.dedup();
            assert_eq!(slots.len(), len, "duplicate slot among {keys:?}");
            assert!(slots.iter().all(|&slot| slot < 64), "a set of keys read is a u64");
        };
        slots(definitions, &|key| {
            definition::read(key).map_or(u8::MAX, |hit| hit.slot)
        });
        slots(schemas, &|key| schema::read(key).map_or(u8::MAX, |hit| hit.slot));
    }

    #[test]
    fn other_keys_are_not_native() {
        for key in [
            "x-other-method-name",
            "x-other-skip",
            "x-custom",
            "summary",
            "x-forge-nope",
            "x-forge-method",
            // What other systems write for a native key, which is not read.
            "x-sensitive",
        ] {
            assert!(definition::read(key).is_none(), "{key}");
            assert!(parameter::read(key).is_none(), "{key}");
            assert!(schema::read(key).is_none(), "{key}");
            assert!(document::read(key).is_none(), "{key}");
        }
    }

    #[test]
    fn aliases_are_not_a_definition_key() {
        assert!(is_aliases("x-forge-aliases"));
        assert!(definition::read("x-forge-aliases").is_none());
    }

    #[test]
    fn availability_is_one_of_five_spellings_and_case_matters() {
        for (value, availability) in [
            ("alpha", Availability::Alpha),
            ("beta", Availability::Beta),
            ("preview", Availability::Preview),
            ("generally-available", Availability::GenerallyAvailable),
            ("deprecated", Availability::Deprecated),
        ] {
            assert_eq!(super::availability(value), Some(availability));
        }
        assert_eq!(super::availability("Beta"), None);
        assert_eq!(super::availability("ga"), None);
    }
}
