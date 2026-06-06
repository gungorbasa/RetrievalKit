use std::collections::BTreeMap;

pub type Metadata = BTreeMap<String, MetadataValue>;

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    TimestampMillis(i64),
}

impl MetadataValue {
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

pub(crate) fn estimated_metadata_payload_bytes(metadata: &Metadata) -> usize {
    metadata
        .iter()
        .map(|(field, value)| field.len() + value.estimated_payload_bytes())
        .sum()
}
