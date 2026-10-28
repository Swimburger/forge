//! The adapter for Stainless's `x-stainless-*` extensions.
//!
//! `x-stainless-skip` may be a list of the targets to skip a property for,
//! which a native skip, that is for every target, does not say.

/// What every key of the adapter begins with. A key without it is turned
/// away before it is matched against the table.
const PREFIX: &str = "x-stainless-";

/// The keys of a Schema Object.
pub(super) mod schema {
    use super::PREFIX;
    use crate::metadata::SchemaField;
    use crate::normalize::vendor::Key;

    const SKIP: &str = "x-stainless-skip";

    /// The names of the adapter's keys of a Schema Object.
    #[cfg(test)]
    pub(in crate::normalize::vendor) const NAMES: [&str; 1] = [SKIP];

    pub(in crate::normalize::vendor) fn read(key: &str) -> Option<Key<SchemaField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            SKIP => Key::bool_or_targets(SchemaField::Skip),
            _ => return None,
        })
    }
}
