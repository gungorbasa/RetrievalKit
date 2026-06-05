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
}
