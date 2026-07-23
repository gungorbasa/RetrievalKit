use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, RetrievalKitError};

const MAX_CORPUS_ID_BYTES: usize = 128;
const MAX_RECORD_ID_BYTES: usize = 512;
const MAX_CHUNK_KEY_BYTES: usize = 512;
const MAX_IDENTIFIER_BYTES: usize = 64;

macro_rules! byte_exact_id {
    ($name:ident, $kind:literal, $max:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                validate_byte_exact_id($kind, &value, $max)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

byte_exact_id!(CorpusId, "CorpusId", MAX_CORPUS_ID_BYTES);
byte_exact_id!(RecordId, "RecordId", MAX_RECORD_ID_BYTES);
byte_exact_id!(ChunkKey, "ChunkKey", MAX_CHUNK_KEY_BYTES);

/// Stable external identity of one retrievable chunk within a record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChunkIdentity {
    pub record_id: RecordId,
    pub chunk_key: ChunkKey,
}

impl ChunkIdentity {
    pub fn new(record_id: RecordId, chunk_key: ChunkKey) -> Self {
        Self {
            record_id,
            chunk_key,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GenerationId(u64);

impl Default for GenerationId {
    fn default() -> Self {
        Self::INITIAL
    }
}

impl GenerationId {
    pub const INITIAL: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordType(String);

impl RecordType {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_identifier("RecordType", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldName(String);

impl FieldName {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_identifier("FieldName", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecordValue {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    List(Vec<RecordValue>),
    Map(BTreeMap<FieldName, RecordValue>),
}

impl RecordValue {
    fn validate(&self, path: &str) -> Result<()> {
        match self {
            Self::F64(value) if !value.is_finite() => Err(RetrievalKitError::InvalidRecordValue {
                path: path.to_owned(),
                message: "non-finite floating-point values are not canonical".to_owned(),
            }),
            Self::List(values) => {
                for (index, value) in values.iter().enumerate() {
                    value.validate(&format!("{path}[{index}]"))?;
                }
                Ok(())
            }
            Self::Map(values) => {
                for (field, value) in values {
                    value.validate(&format!("{path}.{}", field.as_str()))?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn estimated_payload_bytes(&self) -> usize {
        match self {
            Self::Null => 0,
            Self::Bool(_) => std::mem::size_of::<bool>(),
            Self::I64(_) | Self::F64(_) => std::mem::size_of::<u64>(),
            Self::String(value) => value.len(),
            Self::List(values) => values.iter().map(Self::estimated_payload_bytes).sum(),
            Self::Map(values) => values
                .iter()
                .map(|(field, value)| field.as_str().len() + value.estimated_payload_bytes())
                .sum(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: RecordId,
    pub record_type: RecordType,
    pub fields: BTreeMap<FieldName, RecordValue>,
    pub content: Option<String>,
}

impl Record {
    pub fn validate(&self) -> Result<()> {
        for (field, value) in &self.fields {
            value.validate(field.as_str())?;
        }
        Ok(())
    }

    pub(crate) fn estimated_payload_bytes(&self) -> usize {
        self.id.as_str().len()
            + self.record_type.as_str().len()
            + self.content.as_ref().map_or(0, String::len)
            + self
                .fields
                .iter()
                .map(|(field, value)| field.as_str().len() + value.estimated_payload_bytes())
                .sum::<usize>()
    }
}

/// Canonical graph-neutral source records for one local corpus.
///
/// Search indexes and future graph adjacency are derived from this store; they
/// must not become independently editable payload owners.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordStore {
    corpus_id: CorpusId,
    records: BTreeMap<RecordId, Record>,
}

impl RecordStore {
    pub fn new(corpus_id: CorpusId) -> Self {
        Self {
            corpus_id,
            records: BTreeMap::new(),
        }
    }

    pub fn corpus_id(&self) -> &CorpusId {
        &self.corpus_id
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Inserts or atomically replaces the canonical record with the same ID.
    pub fn upsert(&mut self, record: Record) -> Result<Option<Record>> {
        record.validate()?;
        Ok(self.records.insert(record.id.clone(), record))
    }

    pub fn delete(&mut self, record_id: &RecordId) -> Option<Record> {
        self.records.remove(record_id)
    }

    pub fn get(&self, record_id: &RecordId) -> Option<&Record> {
        self.records.get(record_id)
    }

    /// Preserves input order and reports missing IDs without per-record calls.
    pub fn hydrate<'a>(&'a self, record_ids: &[RecordId]) -> Vec<Option<&'a Record>> {
        record_ids.iter().map(|id| self.records.get(id)).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&RecordId, &Record)> {
        self.records.iter()
    }

    pub fn validate(&self) -> Result<()> {
        for (record_id, record) in &self.records {
            if record_id != &record.id {
                return Err(RetrievalKitError::InvalidRecordValue {
                    path: record_id.as_str().to_owned(),
                    message: "record map key does not match the embedded RecordId".to_owned(),
                });
            }
            record.validate()?;
        }
        Ok(())
    }

    pub(crate) fn estimated_payload_bytes(&self) -> usize {
        self.corpus_id.as_str().len()
            + self
                .records
                .values()
                .map(Record::estimated_payload_bytes)
                .sum::<usize>()
    }
}

fn validate_byte_exact_id(kind: &'static str, value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty() || value.len() > max_bytes {
        return Err(RetrievalKitError::InvalidIdentity {
            kind,
            value: value.to_owned(),
            message: format!("must contain 1-{max_bytes} UTF-8 bytes"),
        });
    }
    Ok(())
}

fn validate_identifier(kind: &'static str, value: &str) -> Result<()> {
    let mut bytes = value.bytes();
    let valid_first = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    let valid_rest = bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if value.len() > MAX_IDENTIFIER_BYTES || !valid_first || !valid_rest {
        return Err(RetrievalKitError::InvalidIdentity {
            kind,
            value: value.to_owned(),
            message: "must match [A-Za-z_][A-Za-z0-9_]{0,63}".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, value: RecordValue) -> Record {
        Record {
            id: RecordId::new(id).unwrap(),
            record_type: RecordType::new("Note").unwrap(),
            fields: BTreeMap::from([(FieldName::new("value").unwrap(), value)]),
            content: Some("canonical text".to_owned()),
        }
    }

    #[test]
    fn record_store_round_trips_nested_values_and_distinguishes_null_from_missing() {
        let corpus = CorpusId::new("customer-corpus").unwrap();
        let mut store = RecordStore::new(corpus.clone());
        let nested = RecordValue::Map(BTreeMap::from([(
            FieldName::new("items").unwrap(),
            RecordValue::List(vec![RecordValue::Null, RecordValue::I64(7)]),
        )]));
        store.upsert(record("record-1", nested)).unwrap();

        let bytes = serde_json::to_vec(&store).unwrap();
        let loaded: RecordStore = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(loaded, store);
        assert_eq!(loaded.corpus_id(), &corpus);
        assert!(!loaded
            .get(&RecordId::new("record-1").unwrap())
            .unwrap()
            .fields
            .contains_key(&FieldName::new("missing").unwrap()));
    }

    #[test]
    fn record_store_replace_delete_and_bulk_hydration_are_deterministic() {
        let mut store = RecordStore::new(CorpusId::new("corpus").unwrap());
        let id = RecordId::new("same-id").unwrap();
        assert!(store
            .upsert(record("same-id", RecordValue::Bool(false)))
            .unwrap()
            .is_none());
        assert!(store
            .upsert(record("same-id", RecordValue::Bool(true)))
            .unwrap()
            .is_some());

        let missing = RecordId::new("missing").unwrap();
        let hydrated = store.hydrate(&[missing.clone(), id.clone(), id.clone()]);
        assert!(hydrated[0].is_none());
        assert_eq!(hydrated[1], hydrated[2]);
        assert!(store.delete(&id).is_some());
        assert!(store.delete(&id).is_none());
    }

    #[test]
    fn identities_are_byte_exact_and_bounded() {
        assert_ne!(
            RecordId::new("e\u{301}").unwrap(),
            RecordId::new("é").unwrap()
        );
        assert!(CorpusId::new("").is_err());
        assert!(RecordId::new("x".repeat(MAX_RECORD_ID_BYTES + 1)).is_err());
        assert!(ChunkKey::new("x".repeat(MAX_CHUNK_KEY_BYTES)).is_ok());
        assert!(RecordType::new("bad-name").is_err());
        assert!(FieldName::new("9bad").is_err());
    }

    #[test]
    fn non_finite_values_are_rejected_before_replacement() {
        let mut store = RecordStore::new(CorpusId::new("corpus").unwrap());
        store.upsert(record("record", RecordValue::I64(1))).unwrap();
        assert!(store
            .upsert(record("record", RecordValue::F64(f64::NAN)))
            .is_err());
        let expected = record("record", RecordValue::I64(1));
        assert_eq!(
            store.get(&RecordId::new("record").unwrap()),
            Some(&expected)
        );
    }
}
