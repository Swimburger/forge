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

/// What every native key begins with, but the deprecated `x-sensitive`. A key
/// without it is turned away before it is matched against the table: most
/// extensions in a document are no key of any table, and this is the cheap
/// way to say so.
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
    /// Which kind of key it is, which decides whose value is kept when
    /// several keys say the same thing.
    pub(crate) tier: Tier,
    /// The bit of the key in a set of the keys read from an object so far,
    /// which is how a key written twice is found, and the index of its name.
    /// Unique among the keys of one kind of object, native or not: the
    /// native keys count up from zero, and the others from [`COMPAT_SLOTS`].
    pub(crate) slot: u8,
}

/// Which kind of key gave a value, from the weakest to the strongest. When
/// keys of different tiers say the same thing about an object, the value of
/// the strongest is kept, whatever order they are written in.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Tier {
    /// No key has given a value yet.
    #[default]
    Unset,
    /// A key a compatibility layer reads in place of a native one.
    #[cfg_attr(not(any(feature = "compat", test)), expect(dead_code))]
    Compat,
    /// A native key that is kept working under an old name, in place of
    /// the one that replaced it.
    Deprecated,
    /// A native key, under its current name.
    Native,
}

/// The first slot of the keys that are not native, so that slots never
/// collide with the native ones, which count up from zero.
#[cfg_attr(not(any(feature = "compat", test)), expect(dead_code))]
pub(crate) const COMPAT_SLOTS: u8 = 32;

impl<F> Hit<F> {
    /// Returns a native key, in slot `slot`, which is under 32.
    const fn native(field: F, slot: u8) -> Hit<F> {
        Hit {
            field,
            tier: Tier::Native,
            slot,
        }
    }

    /// Returns a deprecated native key, in slot `slot`, which is under 32:
    /// an old name of a native key, read only when the current one is not
    /// written.
    const fn deprecated(field: F, slot: u8) -> Hit<F> {
        Hit {
            field,
            tier: Tier::Deprecated,
            slot,
        }
    }

    /// Returns a key that is not native, which a compatibility layer
    /// reads in place of a native one. It is the `n`th of its kind of
    /// object, counting from zero, `n` under 32.
    #[cfg(feature = "compat")]
    pub(crate) const fn compat(field: F, n: u8) -> Hit<F> {
        Hit {
            field,
            tier: Tier::Compat,
            slot: COMPAT_SLOTS + n,
        }
    }

    /// Returns this key moved `n` slots on, for a compatibility layer that
    /// puts the keys of several adapters one after another.
    #[cfg(feature = "compat")]
    pub(crate) fn after(self, n: u8) -> Hit<F> {
        Hit {
            field: self.field,
            tier: self.tier,
            slot: self.slot + n,
        }
    }
}

/// Returns the name in `names` of the key in `slot`, if it is a native one.
pub(crate) fn native_name(names: &[&'static str], slot: u8) -> Option<&'static str> {
    names.get(usize::from(slot)).copied()
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
    Skip(BoolFormat),
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
    Skip(BoolFormat),
    /// Whether the schema's value is a secret.
    Sensitive,
}

/// What a key says at the root of the document.
#[derive(Clone, Copy)]
pub(crate) enum DocumentField {
    /// The descriptions of the groups.
    Commands,
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

/// Returns whether `key` is the `x-forge-aliases` of an operation.
pub(crate) fn is_aliases(key: &str) -> bool {
    key == ALIASES
}

/// The native keys of an operation, or of an alias in its `x-forge-aliases`.
pub(crate) mod definition {
    use super::{BoolFormat, DefinitionField, Hit, PREFIX};

    const METHOD_NAME: &str = "x-forge-method-name";
    const GROUP_NAME: &str = "x-forge-group-name";
    const SKIP: &str = "x-forge-skip";
    const AVAILABILITY: &str = "x-forge-availability";
    const HIDDEN: &str = "x-forge-hidden";
    const CONFIRMATION: &str = "x-forge-require-confirmation";

    /// The names of the native keys of an operation or an alias, by slot.
    pub(crate) const NAMES: [&str; 6] = [METHOD_NAME, GROUP_NAME, SKIP, AVAILABILITY, HIDDEN, CONFIRMATION];

    /// Returns the native key that says what `field` says.
    #[cfg(feature = "normalize")]
    pub(crate) fn key(field: DefinitionField) -> &'static str {
        match field {
            DefinitionField::Method => METHOD_NAME,
            DefinitionField::Group => GROUP_NAME,
            DefinitionField::Skip(_) => SKIP,
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
            METHOD_NAME => Hit::native(DefinitionField::Method, 0),
            GROUP_NAME => Hit::native(DefinitionField::Group, 1),
            SKIP => Hit::native(DefinitionField::Skip(BoolFormat::Bool), 2),
            AVAILABILITY => Hit::native(DefinitionField::Availability, 3),
            HIDDEN => Hit::native(DefinitionField::Hidden, 4),
            CONFIRMATION => Hit::native(DefinitionField::Confirmation, 5),
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
    #[cfg(feature = "normalize")]
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
            NAME => Hit::native(ParameterField::Name, 0),
            _ => return None,
        })
    }
}

/// The native keys of a Schema Object.
pub(crate) mod schema {
    use super::{BoolFormat, Hit, PREFIX, SchemaField};

    const MODEL_NAME: &str = "x-forge-model-name";
    const PROPERTY_NAME: &str = "x-forge-property-name";
    const SKIP: &str = "x-forge-skip";
    const SENSITIVE: &str = "x-forge-sensitive";
    /// The old name of `x-forge-sensitive`, from before every native key began
    /// with `x-forge-`. Deprecated: it is read only where `x-forge-sensitive` is
    /// not written.
    const DEPRECATED_SENSITIVE: &str = "x-sensitive";

    /// The names of the native keys of a Schema Object, by slot.
    pub(crate) const NAMES: [&str; 5] = [MODEL_NAME, PROPERTY_NAME, SKIP, SENSITIVE, DEPRECATED_SENSITIVE];

    /// Returns the native key that says what `field` says.
    #[cfg(feature = "normalize")]
    pub(crate) fn key(field: SchemaField) -> &'static str {
        match field {
            SchemaField::ModelName => MODEL_NAME,
            SchemaField::PropertyName => PROPERTY_NAME,
            SchemaField::Skip(_) => SKIP,
            SchemaField::Sensitive => SENSITIVE,
        }
    }

    /// Returns what `key` says on a Schema Object, if it is a native key of one.
    pub(crate) fn read(key: &str) -> Option<Hit<SchemaField>> {
        // `x-sensitive` is the one native key outside the prefix.
        if !key.starts_with(PREFIX) && key != DEPRECATED_SENSITIVE {
            return None;
        }
        Some(match key {
            MODEL_NAME => Hit::native(SchemaField::ModelName, 0),
            PROPERTY_NAME => Hit::native(SchemaField::PropertyName, 1),
            SKIP => Hit::native(SchemaField::Skip(BoolFormat::Bool), 2),
            SENSITIVE => Hit::native(SchemaField::Sensitive, 3),
            DEPRECATED_SENSITIVE => Hit::deprecated(SchemaField::Sensitive, 4),
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
            COMMANDS => Hit::native(DocumentField::Commands, 0),
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

/// Asserts that no two of `keys` that `read` reads say the same thing with
/// the same tier. Which key's value is kept is decided by tier alone, so two
/// keys of one tier for one thing would make it depend on the order the
/// keys are written in.
#[cfg(test)]
#[track_caller]
pub(crate) fn assert_one_key_per_tier<F>(keys: &[&str], read: impl Fn(&str) -> Option<Hit<F>>) {
    let mut said: Vec<(std::mem::Discriminant<F>, Tier, &str)> = Vec::new();
    for &key in keys {
        let hit = read(key).unwrap_or_else(|| panic!("{key} is not read"));
        let what = std::mem::discriminant(&hit.field);
        if let Some(&(_, _, other)) = said.iter().find(|&&(w, tier, _)| w == what && tier == hit.tier) {
            panic!("{other} and {key} say the same thing with the same tier");
        }
        said.push((what, hit.tier, key));
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
                "x-sensitive",
            ],
            &["x-forge-commands"],
        ]
    }

    /// Asserts that each of `keys` is read by `read` as a native key whose
    /// slot names it in `names`.
    #[track_caller]
    fn read_as_itself<F>(keys: &[&str], names: &[&'static str], read: fn(&str) -> Option<Hit<F>>) {
        for &key in keys {
            let hit = read(key).unwrap_or_else(|| panic!("{key} is not read"));
            assert_ne!(hit.tier, Tier::Compat, "{key}");
            assert_eq!(native_name(names, hit.slot), Some(key));
        }
        assert_eq!(keys.len(), names.len(), "every name is a key that is read");
    }

    #[test]
    fn each_native_key_is_read_as_itself_and_as_native() {
        let [definitions, parameters, schemas, documents] = native_keys();
        read_as_itself(definitions, &definition::NAMES, definition::read);
        read_as_itself(parameters, &parameter::NAMES, parameter::read);
        read_as_itself(schemas, &schema::NAMES, schema::read);
        read_as_itself(documents, &document::NAMES, document::read);
    }

    #[test]
    fn no_two_native_keys_say_the_same_thing_with_the_same_tier() {
        let [definitions, parameters, schemas, documents] = native_keys();
        assert_one_key_per_tier(definitions, definition::read);
        assert_one_key_per_tier(parameters, parameter::read);
        assert_one_key_per_tier(schemas, schema::read);
        assert_one_key_per_tier(documents, document::read);
    }

    #[cfg(feature = "normalize")]
    #[test]
    fn each_field_is_said_by_the_native_key_that_reads_as_it() {
        use std::mem::discriminant as same;

        for field in [
            DefinitionField::Method,
            DefinitionField::Group,
            DefinitionField::Skip(BoolFormat::Bool),
            DefinitionField::Availability,
            DefinitionField::Hidden,
            DefinitionField::Confirmation,
        ] {
            let hit = definition::read(definition::key(field)).unwrap();
            assert!(same(&hit.field) == same(&field) && hit.tier == Tier::Native);
        }
        let hit = parameter::read(parameter::key(ParameterField::Name)).unwrap();
        assert!(matches!(hit.field, ParameterField::Name) && hit.tier == Tier::Native);
        for field in [
            SchemaField::ModelName,
            SchemaField::PropertyName,
            SchemaField::Skip(BoolFormat::Bool),
            SchemaField::Sensitive,
        ] {
            let hit = schema::read(schema::key(field)).unwrap();
            assert!(same(&hit.field) == same(&field) && hit.tier == Tier::Native);
        }
        assert_eq!(schema::read("x-sensitive").unwrap().tier, Tier::Deprecated);
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
    #[should_panic(expected = "x-forge-sensitive and x-forge-sensitive say the same thing with the same tier")]
    fn one_key_per_tier_finds_two() {
        assert_one_key_per_tier(&["x-forge-sensitive", "x-forge-sensitive"], schema::read);
    }

    #[test]
    fn x_sensitive_is_a_deprecated_name_of_x_forge_sensitive() {
        let (current, old) = (
            schema::read("x-forge-sensitive").unwrap(),
            schema::read("x-sensitive").unwrap(),
        );

        assert!(matches!(
            (current.field, old.field),
            (SchemaField::Sensitive, SchemaField::Sensitive)
        ));
        assert_eq!((current.tier, old.tier), (Tier::Native, Tier::Deprecated));
        assert!(Tier::Native > Tier::Deprecated && Tier::Deprecated > Tier::Compat && Tier::Compat > Tier::Unset);
    }

    #[test]
    fn a_slot_past_the_native_ones_has_no_native_name() {
        assert_eq!(native_name(&definition::NAMES, COMPAT_SLOTS), None);
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
