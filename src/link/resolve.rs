//! Resolving the `x-` keys of the objects in [`openapi`](super::openapi).
//!
//! A key is native, when [`metadata`] reads it, or else, if forge is built
//! with the `compat` feature, one the compatibility layer reads in its place.
//! That is the whole fallback: native first, and the compatibility layer for
//! a key that is not native. This is the only place the layer is called, so
//! leaving it out takes one cfg arm for each kind of object, here.
//!
//! Only `x-` keys are asked about. Every other key is OpenAPI's own, which
//! `openapi` knows.

use serde::de;

use crate::metadata::{self, DefinitionField, DocumentField, Hit, ParameterField, SchemaField};

/// Returns what `key` says on an operation or an alias, if it is a key of
/// one.
pub(super) fn definition(key: &str) -> Option<Hit<DefinitionField>> {
    metadata::definition(key).or_else(|| compat_definition(key))
}

/// Returns what `key` says on a Parameter Object, if it is a key of one.
pub(super) fn parameter(key: &str) -> Option<Hit<ParameterField>> {
    metadata::parameter(key).or_else(|| compat_parameter(key))
}

/// Returns what `key` says on a Schema Object, if it is a key of one.
pub(super) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    metadata::schema(key).or_else(|| compat_schema(key))
}

/// Returns what `key` says at the root of the document, if it is a key of
/// it. No compatibility key says anything there.
pub(super) fn document(key: &str) -> Option<Hit<DocumentField>> {
    metadata::document(key)
}

/// A kind of object with extensions, which can name its keys by slot.
pub(super) trait Kind {
    /// Returns the name of the key in `slot`, as the document writes it.
    fn name(slot: u8) -> &'static str;
}

impl Kind for DefinitionField {
    fn name(slot: u8) -> &'static str {
        metadata::native_name(&metadata::DEFINITION_NAMES, slot).unwrap_or_else(|| compat_definition_name(slot))
    }
}

impl Kind for ParameterField {
    fn name(slot: u8) -> &'static str {
        metadata::native_name(&metadata::PARAMETER_NAMES, slot).unwrap_or_else(|| compat_parameter_name(slot))
    }
}

impl Kind for SchemaField {
    fn name(slot: u8) -> &'static str {
        metadata::native_name(&metadata::SCHEMA_NAMES, slot).unwrap_or_else(|| compat_schema_name(slot))
    }
}

impl Kind for DocumentField {
    fn name(slot: u8) -> &'static str {
        metadata::native_name(&metadata::DOCUMENT_NAMES, slot).unwrap_or_default()
    }
}

/// Returns an error if the key of `hit` is in `seen`, which has a bit for
/// each key read so far from the object, and else adds it. A native key and
/// a compatibility key of the same meaning are different keys, so both can
/// be written: the native one's value is kept.
pub(super) fn once<F: Kind, E: de::Error>(hit: &Hit<F>, seen: &mut u64) -> Result<(), E> {
    let bit = 1u64 << hit.slot;
    if *seen & bit != 0 {
        return Err(E::duplicate_field(F::name(hit.slot)));
    }
    *seen |= bit;
    Ok(())
}

#[cfg(feature = "compat")]
fn compat_definition(key: &str) -> Option<Hit<DefinitionField>> {
    crate::compat::definition(key)
}

#[cfg(not(feature = "compat"))]
fn compat_definition(_: &str) -> Option<Hit<DefinitionField>> {
    None
}

#[cfg(feature = "compat")]
fn compat_parameter(key: &str) -> Option<Hit<ParameterField>> {
    crate::compat::parameter(key)
}

#[cfg(not(feature = "compat"))]
fn compat_parameter(_: &str) -> Option<Hit<ParameterField>> {
    None
}

#[cfg(feature = "compat")]
fn compat_schema(key: &str) -> Option<Hit<SchemaField>> {
    crate::compat::schema(key)
}

#[cfg(not(feature = "compat"))]
fn compat_schema(_: &str) -> Option<Hit<SchemaField>> {
    None
}

#[cfg(feature = "compat")]
fn compat_definition_name(slot: u8) -> &'static str {
    crate::compat::definition_name(slot - metadata::COMPAT_SLOTS)
}

#[cfg(not(feature = "compat"))]
fn compat_definition_name(_: u8) -> &'static str {
    ""
}

#[cfg(feature = "compat")]
fn compat_parameter_name(slot: u8) -> &'static str {
    crate::compat::parameter_name(slot - metadata::COMPAT_SLOTS)
}

#[cfg(not(feature = "compat"))]
fn compat_parameter_name(_: u8) -> &'static str {
    ""
}

#[cfg(feature = "compat")]
fn compat_schema_name(slot: u8) -> &'static str {
    crate::compat::schema_name(slot - metadata::COMPAT_SLOTS)
}

#[cfg(not(feature = "compat"))]
fn compat_schema_name(_: u8) -> &'static str {
    ""
}

/// Returns the name of the key of an operation or an alias in `slot`.
pub(super) fn definition_name(slot: u8) -> &'static str {
    DefinitionField::name(slot)
}
