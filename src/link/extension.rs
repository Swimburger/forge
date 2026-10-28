//! The native extension keys of the objects in [`openapi`](super::openapi):
//! how to name one from its slot, and how to find one written twice.
//!
//! What a key says is [`metadata`]'s to say. Only `x-` keys are asked about:
//! every other key is OpenAPI's own, which `openapi` knows.

use serde::de;

use crate::metadata::{self, DefinitionField, DocumentField, Hit, ParameterField, SchemaField};

/// A kind of object with extensions, which can name its keys by slot.
pub(super) trait Kind {
    /// Returns the name of the key in `slot`, as the document writes it.
    fn name(slot: u8) -> &'static str;
}

impl Kind for DefinitionField {
    fn name(slot: u8) -> &'static str {
        metadata::definition::NAMES[usize::from(slot)]
    }
}

impl Kind for ParameterField {
    fn name(slot: u8) -> &'static str {
        metadata::parameter::NAMES[usize::from(slot)]
    }
}

impl Kind for SchemaField {
    fn name(slot: u8) -> &'static str {
        metadata::schema::NAMES[usize::from(slot)]
    }
}

impl Kind for DocumentField {
    fn name(slot: u8) -> &'static str {
        metadata::document::NAMES[usize::from(slot)]
    }
}

/// Returns an error if the key of `hit` is in `seen`, which has a bit for
/// each key read so far from the object, and else adds it.
pub(super) fn once<F: Kind, E: de::Error>(hit: &Hit<F>, seen: &mut u64) -> Result<(), E> {
    let bit = 1u64 << hit.slot;
    if *seen & bit != 0 {
        return Err(E::duplicate_field(F::name(hit.slot)));
    }
    *seen |= bit;
    Ok(())
}

/// Returns the name of the key of an operation or an alias in `slot`.
pub(super) fn definition_name(slot: u8) -> &'static str {
    DefinitionField::name(slot)
}
