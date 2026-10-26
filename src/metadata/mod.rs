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
//! and what goes out is small values, including the formats a key's value is
//! written in, as data. Reading the values, and building a `Forge` from
//! them, is the work of `link`, which asks this module first and a
//! compatibility layer, if forge is built with one, second; so a native key
//! always has the last word, and the compatibility layer can be left out
//! without changing what any native key means.

use crate::Availability;

/// The `x-forge-aliases` key of an Operation Object. It is not in
/// [`definition`], since an alias cannot have one.
pub(crate) const ALIASES: &str = "x-forge-aliases";

/// A key that says something about the object it is on.
#[derive(Clone, Copy)]
pub(crate) struct Hit<F> {
    /// What the key says.
    pub(crate) field: F,
    /// The key as the document writes it, for errors.
    pub(crate) key: &'static str,
    /// Whether the key is native. A value from a native key is kept over one
    /// from any other, whatever order they are written in.
    pub(crate) native: bool,
    /// The bit of the key in a set of the keys read from an object so far,
    /// which is how a key written twice is found. Unique among the keys of
    /// one kind of object, native or not.
    pub(crate) slot: u8,
}

/// The first slot of the keys that are not native, so that slots never
/// collide with the native ones, which count up from zero.
#[cfg(feature = "compat")]
const COMPAT_SLOTS: u8 = 32;

impl<F> Hit<F> {
    /// Returns a native key, in slot `slot`, which is under 32.
    const fn native(field: F, key: &'static str, slot: u8) -> Hit<F> {
        Hit {
            field,
            key,
            native: true,
            slot,
        }
    }

    /// Returns a key that is not native, which a compatibility layer
    /// reads in place of a native one. It is the `n`th of its kind of
    /// object, counting from zero, `n` under 32.
    #[cfg(feature = "compat")]
    pub(crate) const fn compat(field: F, key: &'static str, n: u8) -> Hit<F> {
        Hit {
            field,
            key,
            native: false,
            slot: COMPAT_SLOTS + n,
        }
    }
}

/// What a key says on an operation, or on an alias of one.
#[derive(Clone, Copy)]
pub(crate) enum DefinitionField {
    /// The name of the method.
    Method,
    /// The group path of the method.
    Group(GroupFormat),
    /// Whether the definition makes no method.
    Ignore(BoolFormat),
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
    Ignore(BoolFormat),
    /// Whether the schema's value is a secret.
    Sensitive,
}

/// What a key says at the root of the document.
#[derive(Clone, Copy)]
pub(crate) enum DocumentField {
    /// The descriptions of the groups.
    Commands,
}

/// How the value of a group path is written.
#[derive(Clone, Copy)]
pub(crate) enum GroupFormat {
    /// An array of group names, top level first.
    Array,
    /// An array of group names, or one string of them separated by
    /// `separator`. The empty string is no group.
    #[cfg_attr(not(feature = "compat"), expect(dead_code))]
    StringOrArray { separator: char },
}

/// How the value of a boolean is written.
#[derive(Clone, Copy)]
pub(crate) enum BoolFormat {
    /// `true` or `false`.
    Bool,
    /// `true` or `false`, or an array of targets, which counts as `false`.
    #[cfg_attr(not(feature = "compat"), expect(dead_code))]
    BoolOrTargets,
}

/// Returns what `key` says on an operation or an alias, if it is a native
/// key of one.
pub(crate) fn definition(key: &str) -> Option<Hit<DefinitionField>> {
    const METHOD: &str = "x-forge-method";
    const GROUP: &str = "x-forge-group";
    const IGNORE: &str = "x-forge-ignore";
    const AVAILABILITY: &str = "x-forge-availability";
    const HIDDEN: &str = "x-forge-hidden";
    const CONFIRMATION: &str = "x-forge-require-confirmation";
    Some(match key {
        METHOD => Hit::native(DefinitionField::Method, METHOD, 0),
        GROUP => Hit::native(DefinitionField::Group(GroupFormat::Array), GROUP, 1),
        IGNORE => Hit::native(DefinitionField::Ignore(BoolFormat::Bool), IGNORE, 2),
        AVAILABILITY => Hit::native(DefinitionField::Availability, AVAILABILITY, 3),
        HIDDEN => Hit::native(DefinitionField::Hidden, HIDDEN, 4),
        CONFIRMATION => Hit::native(DefinitionField::Confirmation, CONFIRMATION, 5),
        _ => return None,
    })
}

/// Returns whether `key` is the `x-forge-aliases` of an operation.
pub(crate) fn is_aliases(key: &str) -> bool {
    key == ALIASES
}

/// Returns what `key` says on a Parameter Object, if it is a native key of
/// one.
pub(crate) fn parameter(key: &str) -> Option<Hit<ParameterField>> {
    const NAME: &str = "x-forge-parameter-name";
    Some(match key {
        NAME => Hit::native(ParameterField::Name, NAME, 0),
        _ => return None,
    })
}

/// Returns what `key` says on a Schema Object, if it is a native key of one.
pub(crate) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    const MODEL_NAME: &str = "x-forge-model-name";
    const PROPERTY_NAME: &str = "x-forge-property-name";
    const IGNORE: &str = "x-forge-ignore";
    const SENSITIVE: &str = "x-sensitive";
    Some(match key {
        MODEL_NAME => Hit::native(SchemaField::ModelName, MODEL_NAME, 0),
        PROPERTY_NAME => Hit::native(SchemaField::PropertyName, PROPERTY_NAME, 1),
        IGNORE => Hit::native(SchemaField::Ignore(BoolFormat::Bool), IGNORE, 2),
        SENSITIVE => Hit::native(SchemaField::Sensitive, SENSITIVE, 3),
        _ => return None,
    })
}

/// Returns what `key` says at the root of the document, if it is a native
/// key of it.
pub(crate) fn document(key: &str) -> Option<Hit<DocumentField>> {
    const COMMANDS: &str = "x-forge-commands";
    Some(match key {
        COMMANDS => Hit::native(DocumentField::Commands, COMMANDS, 0),
        _ => return None,
    })
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

    /// Every native key of each kind of object, as `(key, slot)`.
    fn native_keys() -> [&'static [&'static str]; 4] {
        [
            &[
                "x-forge-method",
                "x-forge-group",
                "x-forge-ignore",
                "x-forge-availability",
                "x-forge-hidden",
                "x-forge-require-confirmation",
            ],
            &["x-forge-parameter-name"],
            &[
                "x-forge-model-name",
                "x-forge-property-name",
                "x-forge-ignore",
                "x-sensitive",
            ],
            &["x-forge-commands"],
        ]
    }

    #[test]
    fn each_native_key_is_read_as_itself_and_as_native() {
        let [definitions, parameters, schemas, documents] = native_keys();
        for &key in definitions {
            let hit = definition(key).unwrap_or_else(|| panic!("{key} is not a definition key"));
            assert_eq!((hit.key, hit.native), (key, true));
        }
        for &key in parameters {
            let hit = parameter(key).unwrap_or_else(|| panic!("{key} is not a parameter key"));
            assert_eq!((hit.key, hit.native), (key, true));
        }
        for &key in schemas {
            let hit = schema(key).unwrap_or_else(|| panic!("{key} is not a schema key"));
            assert_eq!((hit.key, hit.native), (key, true));
        }
        for &key in documents {
            let hit = document(key).unwrap_or_else(|| panic!("{key} is not a document key"));
            assert_eq!((hit.key, hit.native), (key, true));
        }
    }

    #[test]
    fn slots_are_unique_among_the_keys_of_an_object_and_under_the_compat_ones() {
        let [definitions, _, schemas, _] = native_keys();
        let slots = |keys: &[&str], hit: &dyn Fn(&str) -> u8| {
            let mut slots: Vec<u8> = keys.iter().map(|key| hit(key)).collect();
            slots.sort_unstable();
            let len = slots.len();
            slots.dedup();
            assert_eq!(slots.len(), len, "duplicate slot among {keys:?}");
            assert!(slots.iter().all(|&slot| slot < 32), "native slots are under 32");
        };
        slots(definitions, &|key| definition(key).map_or(u8::MAX, |hit| hit.slot));
        slots(schemas, &|key| schema(key).map_or(u8::MAX, |hit| hit.slot));
    }

    #[test]
    fn other_keys_are_not_native() {
        for key in [
            "x-other-method-name",
            "x-other-skip",
            "x-custom",
            "summary",
            "x-forge-nope",
        ] {
            assert!(definition(key).is_none(), "{key}");
            assert!(parameter(key).is_none(), "{key}");
            assert!(schema(key).is_none(), "{key}");
            assert!(document(key).is_none(), "{key}");
        }
    }

    #[test]
    fn aliases_are_not_a_definition_key() {
        assert!(is_aliases("x-forge-aliases"));
        assert!(definition("x-forge-aliases").is_none());
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
