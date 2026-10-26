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
pub(crate) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    fern::schema(key).or_else(|| stainless::schema(key))
}
