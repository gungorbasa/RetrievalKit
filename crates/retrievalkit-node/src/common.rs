use std::collections::BTreeMap;

use napi::bindgen_prelude::Float32Array;
use napi::{Error, Result, Status};
use napi_derive::napi;
use retrievalkit_core::{
    ChunkKey, FieldName, Filter, Metadata, MetadataValue, Record, RecordChunkInput, RecordId,
    RecordType, RecordValue, RetrievalKitError,
};

#[napi(object)]
#[derive(Clone)]
pub struct NativeMetadataValue {
    pub kind: String,
    pub string_value: Option<String>,
    /// Base-10 i64 transport. The TypeScript facade converts this to/from
    /// `bigint`, avoiding JavaScript number rounding above 2^53.
    pub integer_value: Option<String>,
    pub number_value: Option<f64>,
    pub boolean_value: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeMetadataEntry {
    pub field: String,
    pub value: NativeMetadataValue,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeRecordValue {
    pub kind: String,
    pub string_value: Option<String>,
    pub integer_value: Option<String>,
    pub number_value: Option<f64>,
    pub boolean_value: Option<bool>,
    pub list_value: Option<Vec<NativeRecordValue>>,
    pub map_value: Option<Vec<NativeRecordField>>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeRecordField {
    pub field: String,
    pub value: NativeRecordValue,
}

#[napi(object)]
pub struct NativeChunkInput {
    pub key: String,
    pub text: String,
    pub embedding: Option<Float32Array>,
    pub metadata: Vec<NativeMetadataEntry>,
}

#[napi(object)]
pub struct NativeRecordInput {
    pub id: String,
    pub record_type: String,
    pub fields: Vec<NativeRecordField>,
    pub content: Option<String>,
    pub metadata: Vec<NativeMetadataEntry>,
    pub chunks: Vec<NativeChunkInput>,
}

#[derive(Clone)]
pub(crate) struct OwnedRecordInput {
    pub record: Record,
    pub metadata: Metadata,
    pub chunks: Vec<OwnedChunkInput>,
}

#[derive(Clone)]
pub(crate) struct OwnedChunkInput {
    pub key: ChunkKey,
    pub text: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: Metadata,
}

impl NativeRecordInput {
    pub(crate) fn into_owned(self) -> Result<OwnedRecordInput> {
        let record_id = RecordId::new(self.id).map_err(core_error)?;
        let record_type = RecordType::new(self.record_type).map_err(core_error)?;
        let fields = self
            .fields
            .into_iter()
            .map(|entry| {
                Ok((
                    FieldName::new(entry.field).map_err(core_error)?,
                    entry.value.into_core("record field")?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let chunks = self
            .chunks
            .into_iter()
            .map(|chunk| {
                Ok(OwnedChunkInput {
                    key: ChunkKey::new(chunk.key).map_err(core_error)?,
                    text: chunk.text,
                    embedding: chunk.embedding.map(|values| values.to_vec()),
                    metadata: metadata_from_native(chunk.metadata)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(OwnedRecordInput {
            record: Record {
                id: record_id,
                record_type,
                fields,
                content: self.content,
            },
            metadata: metadata_from_native(self.metadata)?,
            chunks,
        })
    }
}

impl NativeRecordValue {
    fn into_core(self, path: &str) -> Result<RecordValue> {
        match self.kind.as_str() {
            "null" => Ok(RecordValue::Null),
            "boolean" => self
                .boolean_value
                .map(RecordValue::Bool)
                .ok_or_else(|| invalid_boundary(path, "boolean_value is required")),
            "integer" => Ok(RecordValue::I64(exact_i64(
                self.integer_value
                    .ok_or_else(|| invalid_boundary(path, "integer_value is required"))?,
                path,
            )?)),
            "float" => {
                let value = self
                    .number_value
                    .ok_or_else(|| invalid_boundary(path, "number_value is required"))?;
                if !value.is_finite() {
                    return Err(invalid_boundary(path, "float values must be finite"));
                }
                Ok(RecordValue::F64(value))
            }
            "string" => self
                .string_value
                .map(RecordValue::String)
                .ok_or_else(|| invalid_boundary(path, "string_value is required")),
            "list" => self
                .list_value
                .ok_or_else(|| invalid_boundary(path, "list_value is required"))?
                .into_iter()
                .enumerate()
                .map(|(index, value)| value.into_core(&format!("{path}[{index}]")))
                .collect::<Result<Vec<_>>>()
                .map(RecordValue::List),
            "map" => self
                .map_value
                .ok_or_else(|| invalid_boundary(path, "map_value is required"))?
                .into_iter()
                .map(|entry| {
                    let field = FieldName::new(entry.field).map_err(core_error)?;
                    let value = entry.value.into_core(field.as_str())?;
                    Ok((field, value))
                })
                .collect::<Result<BTreeMap<_, _>>>()
                .map(RecordValue::Map),
            actual => Err(invalid_boundary(
                path,
                &format!("unsupported record value kind '{actual}'"),
            )),
        }
    }
}

pub(crate) fn metadata_from_native(entries: Vec<NativeMetadataEntry>) -> Result<Metadata> {
    entries
        .into_iter()
        .map(|entry| {
            let field = entry.field;
            let value = match entry.value.kind.as_str() {
                "string" => entry
                    .value
                    .string_value
                    .map(MetadataValue::String)
                    .ok_or_else(|| invalid_boundary(&field, "string_value is required"))?,
                "integer" => MetadataValue::Integer(exact_i64(
                    entry
                        .value
                        .integer_value
                        .ok_or_else(|| invalid_boundary(&field, "integer_value is required"))?,
                    &field,
                )?),
                "float" => {
                    let value = entry
                        .value
                        .number_value
                        .ok_or_else(|| invalid_boundary(&field, "number_value is required"))?;
                    if !value.is_finite() {
                        return Err(invalid_boundary(&field, "float values must be finite"));
                    }
                    MetadataValue::Float(value)
                }
                "boolean" => MetadataValue::Boolean(
                    entry
                        .value
                        .boolean_value
                        .ok_or_else(|| invalid_boundary(&field, "boolean_value is required"))?,
                ),
                "timestamp" => MetadataValue::TimestampMillis(exact_i64(
                    entry
                        .value
                        .integer_value
                        .ok_or_else(|| invalid_boundary(&field, "integer_value is required"))?,
                    &field,
                )?),
                actual => {
                    return Err(invalid_boundary(
                        &field,
                        &format!("unsupported metadata value kind '{actual}'"),
                    ))
                }
            };
            Ok((field, value))
        })
        .collect()
}

pub(crate) fn metadata_to_native(metadata: &Metadata) -> Vec<NativeMetadataEntry> {
    metadata
        .iter()
        .map(|(field, value)| {
            let (kind, string_value, integer_value, number_value, boolean_value) = match value {
                MetadataValue::String(value) => ("string", Some(value.clone()), None, None, None),
                MetadataValue::Integer(value) => {
                    ("integer", None, Some(value.to_string()), None, None)
                }
                MetadataValue::Float(value) => ("float", None, None, Some(*value), None),
                MetadataValue::Boolean(value) => ("boolean", None, None, None, Some(*value)),
                MetadataValue::TimestampMillis(value) => {
                    ("timestamp", None, Some(value.to_string()), None, None)
                }
            };
            NativeMetadataEntry {
                field: field.clone(),
                value: NativeMetadataValue {
                    kind: kind.to_owned(),
                    string_value,
                    integer_value,
                    number_value,
                    boolean_value,
                },
            }
        })
        .collect()
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeFilter {
    pub kind: String,
    pub field: Option<String>,
    pub value: Option<NativeMetadataValue>,
    pub values: Option<Vec<NativeMetadataValue>>,
    pub lower: Option<NativeMetadataValue>,
    pub upper: Option<NativeMetadataValue>,
    pub children: Option<Vec<NativeFilter>>,
}

impl NativeFilter {
    pub(crate) fn into_core(self) -> Result<Filter> {
        let field = || {
            self.field
                .clone()
                .ok_or_else(|| invalid_boundary("filter", "field is required"))
        };
        match self.kind.as_str() {
            "equals" => Ok(Filter::Equals {
                field: field()?,
                value: metadata_scalar(
                    self.value
                        .ok_or_else(|| invalid_boundary("filter", "value is required"))?,
                    "filter value",
                )?,
            }),
            "notEquals" => Ok(Filter::NotEquals {
                field: field()?,
                value: metadata_scalar(
                    self.value
                        .ok_or_else(|| invalid_boundary("filter", "value is required"))?,
                    "filter value",
                )?,
            }),
            "in" => Ok(Filter::In {
                field: field()?,
                values: self
                    .values
                    .ok_or_else(|| invalid_boundary("filter", "values are required"))?
                    .into_iter()
                    .map(|value| metadata_scalar(value, "filter value"))
                    .collect::<Result<Vec<_>>>()?,
            }),
            "range" => Ok(Filter::Range {
                field: field()?,
                lower: self
                    .lower
                    .map(|value| metadata_scalar(value, "lower bound"))
                    .transpose()?,
                upper: self
                    .upper
                    .map(|value| metadata_scalar(value, "upper bound"))
                    .transpose()?,
            }),
            "exists" => Ok(Filter::Exists { field: field()? }),
            "all" => Ok(Filter::All(
                self.children
                    .ok_or_else(|| invalid_boundary("filter", "children are required"))?
                    .into_iter()
                    .map(Self::into_core)
                    .collect::<Result<Vec<_>>>()?,
            )),
            "any" => Ok(Filter::Any(
                self.children
                    .ok_or_else(|| invalid_boundary("filter", "children are required"))?
                    .into_iter()
                    .map(Self::into_core)
                    .collect::<Result<Vec<_>>>()?,
            )),
            actual => Err(invalid_boundary(
                "filter",
                &format!("unsupported filter kind '{actual}'"),
            )),
        }
    }
}

fn metadata_scalar(value: NativeMetadataValue, field: &str) -> Result<MetadataValue> {
    metadata_from_native(vec![NativeMetadataEntry {
        field: field.to_owned(),
        value,
    }])?
    .remove(field)
    .ok_or_else(|| invalid_boundary(field, "value is required"))
}

pub(crate) fn retrieval_chunks(
    input: OwnedRecordInput,
) -> Result<(Record, Metadata, Vec<RecordChunkInput>)> {
    let chunks = input
        .chunks
        .into_iter()
        .map(|chunk| {
            Ok(RecordChunkInput {
                key: chunk.key,
                text: chunk.text,
                embedding: chunk.embedding.ok_or_else(|| {
                    tagged_error(
                        "RK_MISSING_EMBEDDING",
                        "each retrieval chunk requires an embedding; pass a Float32Array for every chunk",
                    )
                })?,
                metadata: chunk.metadata,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((input.record, input.metadata, chunks))
}

pub(crate) fn exact_i64(value: String, path: &str) -> Result<i64> {
    value.parse::<i64>().map_err(|_| {
        invalid_boundary(
            path,
            &format!(
                "integer value '{value}' is outside signed 64-bit range; pass a bigint between {} and {}",
                i64::MIN,
                i64::MAX
            ),
        )
    })
}

pub(crate) fn invalid_boundary(path: &str, message: &str) -> Error {
    tagged_error(
        "RK_INVALID_INPUT",
        &format!("invalid value at '{path}': {message}"),
    )
}

pub(crate) fn tagged_error(code: &str, message: &str) -> Error {
    Error::new(Status::GenericFailure, format!("{code}: {message}"))
}

pub(crate) fn closed_error(product: &str) -> Error {
    tagged_error(
        "RK_CLOSED",
        &format!("{product} has been closed; create or load a new database before calling this operation"),
    )
}

pub(crate) fn state_error(message: &str) -> Error {
    tagged_error("RK_LIFECYCLE", message)
}

pub(crate) fn core_error(error: RetrievalKitError) -> Error {
    let code = match error {
        RetrievalKitError::InvalidDimension { .. } => "RK_DIMENSION",
        RetrievalKitError::MissingEmbedding { .. } => "RK_MISSING_EMBEDDING",
        RetrievalKitError::Persistence { .. }
        | RetrievalKitError::InvalidFormat { .. }
        | RetrievalKitError::CorruptIndex { .. } => "RK_PERSISTENCE",
        RetrievalKitError::StaleGeneration { .. }
        | RetrievalKitError::InvalidCandidateScope { .. } => "RK_STALE_SELECTION",
        RetrievalKitError::InvalidQuery { .. } => "RK_INVALID_QUERY",
        RetrievalKitError::InvalidIdentity { .. }
        | RetrievalKitError::InvalidRecordValue { .. }
        | RetrievalKitError::InvalidRange { .. }
        | RetrievalKitError::UnsupportedVectorEncoding { .. }
        | RetrievalKitError::RetrievalCapabilityUnavailable { .. } => "RK_INVALID_INPUT",
    };
    tagged_error(code, &error.to_string())
}

pub(crate) fn parse_metric(value: &str) -> Result<retrievalkit_core::VectorMetric> {
    match value {
        "cosine" => Ok(retrievalkit_core::VectorMetric::Cosine),
        "dotProduct" => Ok(retrievalkit_core::VectorMetric::DotProduct),
        actual => Err(invalid_boundary(
            "metric",
            &format!("expected 'cosine' or 'dotProduct', got '{actual}'"),
        )),
    }
}

pub(crate) fn parse_encoding(value: &str) -> Result<retrievalkit_core::VectorEncoding> {
    match value {
        "f32" => Ok(retrievalkit_core::VectorEncoding::F32),
        "f16" => Ok(retrievalkit_core::VectorEncoding::F16),
        "bf16" => Ok(retrievalkit_core::VectorEncoding::BF16),
        "i8" => Ok(retrievalkit_core::VectorEncoding::I8ScalarQuantized),
        actual => Err(invalid_boundary(
            "encoding",
            &format!("expected f32, f16, bf16, or i8; got '{actual}'"),
        )),
    }
}
