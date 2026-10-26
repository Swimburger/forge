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
    if !key.starts_with("x-") {
        return None;
    }
    metadata::definition(key).or_else(|| compat_definition(key))
}

/// Returns what `key` says on a Parameter Object, if it is a key of one.
pub(super) fn parameter(key: &str) -> Option<Hit<ParameterField>> {
    if !key.starts_with("x-") {
        return None;
    }
    metadata::parameter(key).or_else(|| compat_parameter(key))
}

/// Returns what `key` says on a Schema Object, if it is a key of one.
pub(super) fn schema(key: &str) -> Option<Hit<SchemaField>> {
    if !key.starts_with("x-") {
        return None;
    }
    metadata::schema(key).or_else(|| compat_schema(key))
}

/// Returns what `key` says at the root of the document, if it is a key of
/// it. No compatibility key says anything there.
pub(super) fn document(key: &str) -> Option<Hit<DocumentField>> {
    if !key.starts_with("x-") {
        return None;
    }
    metadata::document(key)
}

/// Returns an error if the key of `hit` is in `seen`, which has a bit for
/// each key read so far from the object, and else adds it. A native key and
/// a compatibility key of the same meaning are different keys, so both can
/// be written: the native one's value is kept.
pub(super) fn once<F, E: de::Error>(hit: &Hit<F>, seen: &mut u64) -> Result<(), E> {
    let bit = 1u64 << hit.slot;
    if *seen & bit != 0 {
        return Err(E::duplicate_field(hit.key));
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
