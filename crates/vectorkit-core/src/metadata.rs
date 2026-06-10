use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub type Metadata = BTreeMap<String, MetadataValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetadataValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    TimestampMillis(i64),
}

impl MetadataValue {
    /// Creates a string metadata value.
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    /// Creates an integer metadata value.
    pub fn integer(value: i64) -> Self {
        Self::Integer(value)
    }

    /// Creates a floating-point metadata value.
    pub fn float(value: f64) -> Self {
        Self::Float(value)
    }

    /// Creates a boolean metadata value.
    pub fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }

    /// Creates a timestamp metadata value represented as milliseconds.
    pub fn timestamp_millis(value: i64) -> Self {
        Self::TimestampMillis(value)
    }

    pub(crate) fn as_ordered_f64(&self) -> Option<f64> {
        match self {
            Self::Integer(value) | Self::TimestampMillis(value) => Some(*value as f64),
            Self::Float(value) => Some(*value),
            Self::String(_) | Self::Boolean(_) => None,
        }
    }

    pub(crate) fn estimated_payload_bytes(&self) -> usize {
        match self {
            Self::String(value) => value.len(),
            Self::Integer(_) | Self::TimestampMillis(_) => std::mem::size_of::<i64>(),
            Self::Float(_) => std::mem::size_of::<f64>(),
            Self::Boolean(_) => std::mem::size_of::<bool>(),
        }
    }
}

impl From<String> for MetadataValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for MetadataValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<i64> for MetadataValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for MetadataValue {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}

impl From<f64> for MetadataValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<f32> for MetadataValue {
    fn from(value: f32) -> Self {
        Self::Float(f64::from(value))
    }
}

impl From<bool> for MetadataValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

pub(crate) fn estimated_metadata_payload_bytes(metadata: &Metadata) -> usize {
    metadata
        .iter()
        .map(|(field, value)| field.len() + value.estimated_payload_bytes())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_value_constructors_create_typed_values() {
        assert_eq!(
            MetadataValue::string("notes"),
            MetadataValue::String("notes".to_owned())
        );
        assert_eq!(MetadataValue::integer(42), MetadataValue::Integer(42));
        assert_eq!(MetadataValue::float(1.5), MetadataValue::Float(1.5));
        assert_eq!(MetadataValue::boolean(true), MetadataValue::Boolean(true));
        assert_eq!(
            MetadataValue::timestamp_millis(1_700_000_000),
            MetadataValue::TimestampMillis(1_700_000_000)
        );
    }

    #[test]
    fn metadata_value_from_primitives_uses_expected_types() {
        assert_eq!(
            MetadataValue::from("notes"),
            MetadataValue::String("notes".to_owned())
        );
        assert_eq!(
            MetadataValue::from(String::from("notes")),
            MetadataValue::String("notes".to_owned())
        );
        assert_eq!(MetadataValue::from(5_i64), MetadataValue::Integer(5));
        assert_eq!(MetadataValue::from(5_i32), MetadataValue::Integer(5));
        assert_eq!(MetadataValue::from(1.25_f64), MetadataValue::Float(1.25));
        assert_eq!(MetadataValue::from(1.25_f32), MetadataValue::Float(1.25));
        assert_eq!(MetadataValue::from(false), MetadataValue::Boolean(false));
    }
}
