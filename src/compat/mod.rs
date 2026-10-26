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
    fern::definition(key)
}

/// Returns what `key` says on a Parameter Object, if an adapter reads it
/// there.
pub(crate) fn parameter(key: &str) -> Option<Hit<ParameterField>> {
    fern::parameter(key)
}

/// Returns what `key` says on a Schema Object, if an adapter reads it there.
/// Each adapter counts its keys from zero, so Stainless's come after Fern's.
pub(crate) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    fern::schema(key).or_else(|| stainless::schema(key).map(|hit| hit.after(FERN_SCHEMA_KEYS)))
}

/// How many keys of a Schema Object Fern has, which is where Stainless's
/// start.
const FERN_SCHEMA_KEYS: u8 = fern::SCHEMA_NAMES.len() as u8;

/// Returns the name of the key of an operation or an alias that is `n`th
/// among the keys of the adapters.
pub(crate) fn definition_name(n: u8) -> &'static str {
    fern::DEFINITION_NAMES[usize::from(n)]
}

/// Returns the name of the key of a Parameter Object that is `n`th among the
/// keys of the adapters.
pub(crate) fn parameter_name(n: u8) -> &'static str {
    fern::PARAMETER_NAMES[usize::from(n)]
}

/// Returns the name of the key of a Schema Object that is `n`th among the
/// keys of the adapters.
pub(crate) fn schema_name(n: u8) -> &'static str {
    match n.checked_sub(FERN_SCHEMA_KEYS) {
        None => fern::SCHEMA_NAMES[usize::from(n)],
        Some(n) => stainless::SCHEMA_NAMES[usize::from(n)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        read_as_itself(&fern::DEFINITION_NAMES, definition, definition_name);
        read_as_itself(&fern::PARAMETER_NAMES, parameter, parameter_name);
        read_as_itself(&fern::SCHEMA_NAMES, schema, schema_name);
        read_as_itself(&stainless::SCHEMA_NAMES, schema, schema_name);
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
        let definitions = keys(&metadata::DEFINITION_NAMES, &[&fern::DEFINITION_NAMES]);
        let parameters = keys(&metadata::PARAMETER_NAMES, &[&fern::PARAMETER_NAMES]);
        let schemas = keys(
            &metadata::SCHEMA_NAMES,
            &[&fern::SCHEMA_NAMES, &stainless::SCHEMA_NAMES],
        );

        metadata::assert_one_key_per_tier(&definitions, |key| {
            metadata::definition(key).or_else(|| definition(key))
        });
        metadata::assert_one_key_per_tier(&parameters, |key| metadata::parameter(key).or_else(|| parameter(key)));
        metadata::assert_one_key_per_tier(&schemas, |key| metadata::schema(key).or_else(|| schema(key)));
    }

    #[test]
    fn the_slots_of_the_adapters_do_not_collide_and_fit_in_a_set() {
        let mut slots: Vec<u8> = fern::SCHEMA_NAMES
            .iter()
            .chain(&stainless::SCHEMA_NAMES)
            .map(|key| schema(key).unwrap().slot)
            .collect();
        let len = slots.len();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), len);
        assert!(slots.iter().all(|&slot| slot < 64));
    }
}
