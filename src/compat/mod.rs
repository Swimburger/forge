//! The compatibility layer: the keys other tools' metadata is written with,
//! read as the native extensions of [`metadata`](crate::metadata) say the
//! same things.
//!
//! Each adapter, [`fern`] and [`stainless`], maps its tool's keys onto the
//! native primitives and says, as data, what format each value is written
//! in. This module is what `link` asks, once per kind of object, after the
//! native keys have said no. It is the only way in: every key an adapter
//! reads is private to it.
//!
//! The layer is built only with the `compat` feature, and is meant to go:
//! once nobody writes these keys, deleting this folder and the calls to it in
//! `link/resolve.rs` changes nothing about what any native extension means. The keys are read in place of a native key only when there is none,
//! as `link` decides.

mod fern;
mod stainless;

use crate::metadata::{DefinitionField, Hit, ParameterField, SchemaField};

/// Returns what `key` says on an operation or an alias, if an adapter reads
/// it there.
pub(crate) fn definition(key: &str) -> Option<Hit<DefinitionField>> {
    fern::definition::read(key)
}

/// Returns what `key` says on a Parameter Object, if an adapter reads it
/// there.
pub(crate) fn parameter(key: &str) -> Option<Hit<ParameterField>> {
    fern::parameter::read(key)
}

/// Returns what `key` says on a Schema Object, if an adapter reads it there.
/// Each adapter counts its keys from zero, so Stainless's come after Fern's.
pub(crate) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    fern::schema::read(key).or_else(|| stainless::schema::read(key).map(|hit| hit.after(FERN_SCHEMA_KEYS)))
}

/// How many keys of a Schema Object Fern has, which is where Stainless's
/// start.
const FERN_SCHEMA_KEYS: u8 = fern::schema::NAMES.len() as u8;

/// Returns the name of the key of an operation or an alias that is `n`th
/// among the keys of the adapters.
pub(crate) fn definition_name(n: u8) -> &'static str {
    fern::definition::NAMES[usize::from(n)]
}

/// Returns the name of the key of a Parameter Object that is `n`th among the
/// keys of the adapters.
pub(crate) fn parameter_name(n: u8) -> &'static str {
    fern::parameter::NAMES[usize::from(n)]
}

/// Returns the name of the key of a Schema Object that is `n`th among the
/// keys of the adapters.
pub(crate) fn schema_name(n: u8) -> &'static str {
    match n.checked_sub(FERN_SCHEMA_KEYS) {
        None => fern::schema::NAMES[usize::from(n)],
        Some(n) => stainless::schema::NAMES[usize::from(n)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names of the keys of a level, and whether the level reads a key.
    type Level<'a> = (&'a [&'a str], fn(&str) -> bool);
    use crate::metadata::{self, COMPAT_SLOTS, Tier};

    /// Asserts that each key in `names` is read by `read` as a key that is
    /// not native, whose slot `name` names it by.
    #[track_caller]
    fn read_as_itself<F>(names: &[&'static str], read: fn(&str) -> Option<Hit<F>>, name: fn(u8) -> &'static str) {
        for &key in names {
            let hit = read(key).unwrap_or_else(|| panic!("{key} is not read"));
            assert_eq!(hit.tier, Tier::Compat, "{key}");
            assert_eq!(name(hit.slot - COMPAT_SLOTS), key);
        }
    }

    #[test]
    fn each_key_of_each_adapter_is_named_by_its_slot() {
        read_as_itself(&fern::definition::NAMES, definition, definition_name);
        read_as_itself(&fern::parameter::NAMES, parameter, parameter_name);
        read_as_itself(&fern::schema::NAMES, schema, schema_name);
        read_as_itself(&stainless::schema::NAMES, schema, schema_name);
    }

    #[test]
    fn a_key_of_an_adapter_is_read_only_at_its_level() {
        let definitions = fern::definition::NAMES.to_vec();
        let parameters = fern::parameter::NAMES.to_vec();
        let schemas = [&fern::schema::NAMES[..], &stainless::schema::NAMES[..]].concat();
        let levels: [Level; 3] = [
            (&definitions, |key| definition(key).is_some()),
            (&parameters, |key| parameter(key).is_some()),
            (&schemas, |key| schema(key).is_some()),
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
    fn no_two_keys_say_the_same_thing_with_the_same_tier() {
        // As `link` reads them: native first, then the adapters.
        let keys = |native: &[&'static str], adapters: &[&[&'static str]]| -> Vec<&'static str> {
            native
                .iter()
                .chain(adapters.iter().flat_map(|keys| keys.iter()))
                .copied()
                .collect()
        };
        let definitions = keys(&metadata::definition::NAMES, &[&fern::definition::NAMES]);
        let parameters = keys(&metadata::parameter::NAMES, &[&fern::parameter::NAMES]);
        let schemas = keys(
            &metadata::schema::NAMES,
            &[&fern::schema::NAMES, &stainless::schema::NAMES],
        );

        metadata::assert_one_key_per_tier(&definitions, |key| {
            metadata::definition::read(key).or_else(|| definition(key))
        });
        metadata::assert_one_key_per_tier(&parameters, |key| {
            metadata::parameter::read(key).or_else(|| parameter(key))
        });
        metadata::assert_one_key_per_tier(&schemas, |key| metadata::schema::read(key).or_else(|| schema(key)));
    }

    #[test]
    fn the_slots_of_the_adapters_do_not_collide_and_fit_in_a_set() {
        let mut slots: Vec<u8> = fern::schema::NAMES
            .iter()
            .chain(&stainless::schema::NAMES)
            .map(|key| schema(key).unwrap().slot)
            .collect();
        let len = slots.len();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), len);
        assert!(slots.iter().all(|&slot| slot < 64));
    }
}
