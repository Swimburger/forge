//! The keys other tools' metadata is written with, and the native extension of
//! [`metadata`](crate::metadata) that each says the same as. This is the only
//! code that holds them: `forge` itself reads native keys and no others.
//!
//! Each adapter, [`fern`] and [`stainless`], maps its tool's keys onto native
//! fields, and says whether the value is written as the native key's is. The
//! names other systems write without the `x-forge-` prefix are in
//! [`unprefixed`]. This module asks them, once per kind of object.
//!
//! A key has to be read at the kind of object `forge` reads its native key
//! at, so that a rewrite does not give a key a meaning it did not have. An
//! extension that says something `forge` has no primitive for is not here.

mod fern;
mod stainless;
#[cfg(test)]
mod tests;
mod unprefixed;

use crate::metadata::{DefinitionField, ParameterField, SchemaField};

/// What a key of another tool says, and how its value is written.
#[derive(Clone, Copy)]
pub(super) struct Key<F> {
    /// The native field that says the same.
    pub(super) field: F,
    /// Whether the value may be a list of targets, as well as a boolean. A
    /// list of targets is not something a native key says.
    pub(super) targets: bool,
}

impl<F> Key<F> {
    /// Returns a key whose value is written as the native key's is.
    const fn same(field: F) -> Key<F> {
        Key { field, targets: false }
    }

    /// Returns a key whose value is a boolean, or a list of targets.
    const fn bool_or_targets(field: F) -> Key<F> {
        Key { field, targets: true }
    }
}

/// Returns what `key` says on an operation or an alias, if an adapter reads
/// it there.
pub(super) fn definition(key: &str) -> Option<Key<DefinitionField>> {
    fern::definition::read(key)
}

/// Returns what `key` says on a Parameter Object, if an adapter reads it
/// there.
pub(super) fn parameter(key: &str) -> Option<Key<ParameterField>> {
    fern::parameter::read(key)
}

/// Returns what `key` says on a Schema Object, if an adapter reads it there.
pub(super) fn schema(key: &str) -> Option<Key<SchemaField>> {
    fern::schema::read(key)
        .or_else(|| stainless::schema::read(key))
        .or_else(|| unprefixed::schema::read(key))
}
