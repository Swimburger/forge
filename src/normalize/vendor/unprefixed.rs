//! The names other systems write native keys under, without the `x-forge-`
//! prefix. They are in use, so they are not deprecated: they are rewritten.

/// The keys of a Schema Object.
pub(super) mod schema {
    use crate::metadata::SchemaField;
    use crate::normalize::vendor::Key;

    /// What other systems write for `x-forge-sensitive`.
    const SENSITIVE: &str = "x-sensitive";

    /// The names of the keys of a Schema Object.
    #[cfg(test)]
    pub(in crate::normalize::vendor) const NAMES: [&str; 1] = [SENSITIVE];

    pub(in crate::normalize::vendor) fn read(key: &str) -> Option<Key<SchemaField>> {
        match key {
            SENSITIVE => Some(Key::same(SchemaField::Sensitive)),
            _ => None,
        }
    }
}
