//! The adapter for Fern's `x-fern-*` extensions.
//!
//! A group path written as one string is split on dots, as for
//! `x-forge-group-name`, which Fern itself does not do: Fern reads a string as a
//! single group. A document written for Fern uses the array, which means the
//! same in both.

/// What every key of the adapter begins with. A key without it is turned
/// away before it is matched against the table.
const PREFIX: &str = "x-fern-";

/// The keys of an operation or an alias.
pub(super) mod definition {
    use super::PREFIX;
    use crate::metadata::DefinitionField;
    use crate::normalize::vendor::Key;

    const METHOD_NAME: &str = "x-fern-sdk-method-name";
    const GROUP_NAME: &str = "x-fern-sdk-group-name";
    const IGNORE: &str = "x-fern-ignore";
    const AVAILABILITY: &str = "x-fern-availability";

    /// The names of the adapter's keys of an operation or an alias.
    #[cfg(test)]
    pub(in crate::normalize::vendor) const NAMES: [&str; 4] = [METHOD_NAME, GROUP_NAME, IGNORE, AVAILABILITY];

    pub(in crate::normalize::vendor) fn read(key: &str) -> Option<Key<DefinitionField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            METHOD_NAME => Key::same(DefinitionField::Method),
            GROUP_NAME => Key::same(DefinitionField::Group),
            IGNORE => Key::same(DefinitionField::Skip),
            AVAILABILITY => Key::same(DefinitionField::Availability),
            _ => return None,
        })
    }
}

/// The keys of a Parameter Object.
pub(super) mod parameter {
    use super::PREFIX;
    use crate::metadata::ParameterField;
    use crate::normalize::vendor::Key;

    const NAME: &str = "x-fern-parameter-name";

    /// The names of the adapter's keys of a Parameter Object.
    #[cfg(test)]
    pub(in crate::normalize::vendor) const NAMES: [&str; 1] = [NAME];

    pub(in crate::normalize::vendor) fn read(key: &str) -> Option<Key<ParameterField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            NAME => Key::same(ParameterField::Name),
            _ => return None,
        })
    }
}

/// The keys of a Schema Object.
pub(super) mod schema {
    use super::PREFIX;
    use crate::metadata::SchemaField;
    use crate::normalize::vendor::Key;

    const TYPE_NAME: &str = "x-fern-type-name";
    const PROPERTY_NAME: &str = "x-fern-property-name";

    /// The names of the adapter's keys of a Schema Object.
    #[cfg(test)]
    pub(in crate::normalize::vendor) const NAMES: [&str; 2] = [TYPE_NAME, PROPERTY_NAME];

    pub(in crate::normalize::vendor) fn read(key: &str) -> Option<Key<SchemaField>> {
        if !key.starts_with(PREFIX) {
            return None;
        }
        Some(match key {
            TYPE_NAME => Key::same(SchemaField::ModelName),
            PROPERTY_NAME => Key::same(SchemaField::PropertyName),
            _ => return None,
        })
    }
}
