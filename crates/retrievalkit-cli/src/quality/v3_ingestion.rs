use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use retrievalkit_core::{
    ChunkIdentity, ChunkKey, CorpusChunkInput, CorpusId, CorpusIndex, FieldName, Filter,
    IndexConfig, Metadata, MetadataValue, Record, RecordChunkInput, RecordId, RecordInput,
    RecordType, RecordValue, RetrievalConfiguration, RetrievalDatabase, VectorEncoding,
    VectorMetric,
};
use serde_json::Value;

use super::v3_canonical::sha256;
use super::v3_schema::{Query, Record as V3Record};
use super::v3_validation::ValidatedCollection;

const FROZEN_COLLECTION_SHA256: &str =
    "0452e0d1a3bd5d8aed8343fe6aedbcca7c70fab43c8c5edcbc051a930eb89a65";
const FROZEN_RETRIEVAL_POPULATION_SHA256: &str =
    "c373605c9580a90c0194ed28f5e07debfef5f8315547e9af5eb2cae963bfd4e3";
const FROZEN_WHOLE_CORPUS_LOGICAL_RUNS: [(&str, &str); 3] = [
    (
        "a",
        "bf237c1a474816a1f8c8dcb0580694c19ccd53cb5420c99b0419c3dd8bba2711",
    ),
    (
        "b",
        "e0b946e2b8c926badacc6f6fa104d52c33f72f6e8408820f969b59f5d6a6261b",
    ),
    (
        "c",
        "df48c1d3a962997bf21f037c6eae1905ed423576933da54dde749b9170af0b21",
    ),
];

#[derive(Debug, Clone)]
pub(super) struct ProductionRecordInput {
    pub record: Record,
    pub inherited_metadata: Metadata,
    pub chunks: Vec<CorpusChunkInput>,
}

#[derive(Debug, Clone)]
pub(super) struct ProductionQueryInput {
    pub query_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub filter: Option<Filter>,
}

#[derive(Debug, Clone)]
pub(super) struct V3ProductionInputs {
    pub corpus_id: CorpusId,
    pub dimension: usize,
    pub records: Vec<ProductionRecordInput>,
    corpus_embeddings: BTreeMap<ChunkIdentity, Vec<f32>>,
    pub queries: Vec<ProductionQueryInput>,
}

impl V3ProductionInputs {
    pub(super) fn from_validated(validated: &ValidatedCollection) -> Result<Self, String> {
        verify_frozen_contract(validated)?;
        validate_quantization_inputs(validated)?;

        let corpus_embeddings = validated
            .corpus_embeddings
            .iter()
            .map(|row| {
                let record_id = RecordId::new(row.record_id.clone()).map_err(|error| {
                    format!("V3 production ingestion: embedding record ID: {error}")
                })?;
                let chunk_key = ChunkKey::new(row.chunk_key.clone()).map_err(|error| {
                    format!("V3 production ingestion: embedding chunk key: {error}")
                })?;
                Ok((ChunkIdentity::new(record_id, chunk_key), row.values.clone()))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        if corpus_embeddings.len() != validated.corpus_embeddings.len() {
            return Err("V3 production ingestion: duplicate corpus embedding identity".to_owned());
        }

        let records = validated
            .records
            .iter()
            .map(convert_record)
            .collect::<Result<Vec<_>, _>>()?;

        let chunk_identities = records
            .iter()
            .flat_map(|input| {
                input
                    .chunks
                    .iter()
                    .map(|chunk| ChunkIdentity::new(input.record.id.clone(), chunk.key.clone()))
            })
            .collect::<BTreeSet<_>>();
        if chunk_identities.len() != corpus_embeddings.len()
            || chunk_identities
                .iter()
                .any(|identity| !corpus_embeddings.contains_key(identity))
        {
            return Err(
                "V3 production ingestion: corpus and embedding identities do not match".to_owned(),
            );
        }

        let query_embeddings = validated
            .query_embeddings
            .iter()
            .map(|row| (row.query_id.as_str(), row.values.as_slice()))
            .collect::<BTreeMap<_, _>>();
        if query_embeddings.len() != validated.query_embeddings.len() {
            return Err("V3 production ingestion: duplicate query embedding identity".to_owned());
        }
        let queries = validated
            .queries
            .iter()
            .filter(|query| query.tasks.iter().any(|task| task == "retrieval"))
            .map(|query| convert_query(query, &query_embeddings, validated.dimension))
            .collect::<Result<Vec<_>, _>>()?;

        let expected_queries = validated
            .populations
            .retrieval
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let actual_queries = queries
            .iter()
            .map(|query| query.query_id.as_str())
            .collect::<Vec<_>>();
        if actual_queries != expected_queries {
            return Err(format!(
                "V3 production ingestion: retrieval population mismatch; expected {expected_queries:?}, actual {actual_queries:?}"
            ));
        }

        Ok(Self {
            corpus_id: CorpusId::new(validated.collection.corpus_id.clone())
                .map_err(|error| format!("V3 production ingestion: invalid corpus ID: {error}"))?,
            dimension: validated.dimension,
            records,
            corpus_embeddings,
            queries,
        })
    }

    #[cfg(test)]
    pub(super) fn build_corpus(&self) -> Result<CorpusIndex, String> {
        let mut corpus = CorpusIndex::new(self.corpus_id.clone());
        for input in self.canonical_records()? {
            let expected = input
                .chunks
                .iter()
                .map(|chunk| ChunkIdentity::new(input.record.id.clone(), chunk.key.clone()))
                .collect::<Vec<_>>();
            let chunk_ids = corpus
                .upsert(RecordInput {
                    record: input.record.clone(),
                    metadata: input.inherited_metadata.clone(),
                    chunks: input.chunks.clone(),
                })
                .map_err(|error| {
                    format!(
                        "V3 production ingestion: record '{}': {error}",
                        input.record.id.as_str()
                    )
                })?;
            validate_ingested_identities(&corpus, &expected, &chunk_ids)?;
        }
        validate_corpus_shape(&corpus, self.records.len(), self.chunk_count())?;
        Ok(corpus)
    }

    pub(super) fn build_database(
        &self,
        encoding: VectorEncoding,
    ) -> Result<RetrievalDatabase, String> {
        if !matches!(
            encoding,
            VectorEncoding::F32 | VectorEncoding::I8ScalarQuantized
        ) {
            return Err(format!(
                "V3 production ingestion: Phase 1.2a supports only F32 and I8, actual {encoding:?}"
            ));
        }
        let configuration = RetrievalConfiguration::semantic(
            IndexConfig::new(self.dimension, VectorMetric::Cosine).with_vector_encoding(encoding),
        );
        let mut database = RetrievalDatabase::new(configuration, self.corpus_id.clone())
            .map_err(|error| format!("V3 production ingestion: create database: {error}"))?;

        for input in self.canonical_records()? {
            let expected = input
                .chunks
                .iter()
                .map(|chunk| ChunkIdentity::new(input.record.id.clone(), chunk.key.clone()))
                .collect::<Vec<_>>();
            let chunks = input
                .chunks
                .iter()
                .zip(&expected)
                .map(|(chunk, identity)| {
                    let embedding = self.corpus_embeddings.get(identity).ok_or_else(|| {
                        format!(
                            "V3 production ingestion: missing embedding for {}/{}",
                            identity.record_id.as_str(),
                            identity.chunk_key.as_str()
                        )
                    })?;
                    Ok(RecordChunkInput {
                        key: chunk.key.clone(),
                        text: chunk.text.clone(),
                        embedding: embedding.clone(),
                        metadata: chunk.metadata.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let chunk_ids = database
                .upsert_record(
                    input.record.clone(),
                    input.inherited_metadata.clone(),
                    chunks,
                )
                .map_err(|error| {
                    format!(
                        "V3 production ingestion: record '{}': {error}",
                        input.record.id.as_str()
                    )
                })?;
            if chunk_ids.len() != expected.len() {
                return Err(format!(
                    "V3 production ingestion: record '{}' expected {} production chunks, actual {}",
                    input.record.id.as_str(),
                    expected.len(),
                    chunk_ids.len()
                ));
            }
            validate_ingested_identities(database.corpus(), &expected, &chunk_ids)?;
        }

        validate_corpus_shape(database.corpus(), self.records.len(), self.chunk_count())?;
        if database.retrieval().vector_encoding() != encoding
            || database.retrieval().dimension() != self.dimension
            || database.retrieval().metric() != VectorMetric::Cosine
        {
            return Err("V3 production ingestion: built database failed dimension, metric, or encoding validation".to_owned());
        }
        Ok(database)
    }

    fn chunk_count(&self) -> usize {
        self.records.iter().map(|record| record.chunks.len()).sum()
    }

    fn canonical_records(&self) -> Result<Vec<ProductionRecordInput>, String> {
        let mut records = self.records.clone();
        records.sort_by(|left, right| left.record.id.cmp(&right.record.id));
        for record in &mut records {
            record
                .chunks
                .sort_by(|left, right| left.key.cmp(&right.key));
            if record
                .chunks
                .windows(2)
                .any(|pair| pair[0].key == pair[1].key)
            {
                return Err(format!(
                    "V3 production ingestion: duplicate stable chunk key in record '{}'",
                    record.record.id.as_str()
                ));
            }
        }
        if records
            .windows(2)
            .any(|pair| pair[0].record.id == pair[1].record.id)
        {
            return Err("V3 production ingestion: duplicate stable record identity".to_owned());
        }
        Ok(records)
    }
}

pub(super) fn build_graph_corpus(validated: &ValidatedCollection) -> Result<CorpusIndex, String> {
    let mut records = validated
        .records
        .iter()
        .map(convert_record)
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| left.record.id.cmp(&right.record.id));
    let mut corpus = CorpusIndex::new(
        CorpusId::new(validated.collection.corpus_id.clone())
            .map_err(|error| format!("V3 graph ingestion: invalid corpus ID: {error}"))?,
    );
    for input in records {
        corpus
            .upsert(RecordInput {
                record: input.record,
                metadata: input.inherited_metadata,
                chunks: input.chunks,
            })
            .map_err(|error| format!("V3 graph ingestion: canonical record upsert: {error}"))?;
    }
    validate_corpus_shape(
        &corpus,
        validated.records.len(),
        validated
            .records
            .iter()
            .map(|record| record.chunks.len())
            .sum(),
    )?;
    Ok(corpus)
}

fn validate_ingested_identities(
    corpus: &CorpusIndex,
    expected: &[ChunkIdentity],
    chunk_ids: &[u64],
) -> Result<(), String> {
    if chunk_ids.len() != expected.len() {
        return Err(format!(
            "V3 production ingestion: expected {} production chunks, actual {}",
            expected.len(),
            chunk_ids.len()
        ));
    }
    for (identity, chunk_id) in expected.iter().zip(chunk_ids) {
        if corpus.chunk_id_for_identity(identity) != Some(*chunk_id)
            || corpus.chunk_identity(*chunk_id) != Some(identity)
            || corpus
                .chunk(*chunk_id)
                .is_none_or(|chunk| chunk.document_id != identity.record_id.as_str())
        {
            return Err(format!(
                "V3 production ingestion: production identity mismatch for {}/{}",
                identity.record_id.as_str(),
                identity.chunk_key.as_str()
            ));
        }
    }
    Ok(())
}

fn validate_corpus_shape(
    corpus: &CorpusIndex,
    expected_records: usize,
    expected_chunks: usize,
) -> Result<(), String> {
    if corpus.record_store().len() != expected_records
        || corpus.active_chunk_count() != expected_chunks
        || corpus.generation().get() != expected_records as u64
    {
        return Err(
            "V3 production ingestion: built corpus failed record, chunk, or generation validation"
                .to_owned(),
        );
    }
    Ok(())
}

fn verify_frozen_contract(validated: &ValidatedCollection) -> Result<(), String> {
    if validated.collection.collection_id != "vectorkit-v3-conformance" {
        return Ok(());
    }
    let collection_bytes = fs::read(validated.root.join("collection.json"))
        .map_err(|error| format!("V3 production ingestion: reread collection.json: {error}"))?;
    let collection_hash = sha256(&collection_bytes);
    if collection_hash != FROZEN_COLLECTION_SHA256 {
        return Err(format!(
            "V3 production ingestion: frozen collection hash expected {FROZEN_COLLECTION_SHA256}, actual {collection_hash}"
        ));
    }
    let retrieval_hash = super::v3_population::population_hash(&validated.populations.retrieval);
    if retrieval_hash != FROZEN_RETRIEVAL_POPULATION_SHA256 {
        return Err(format!(
            "V3 production ingestion: frozen retrieval population hash expected {FROZEN_RETRIEVAL_POPULATION_SHA256}, actual {retrieval_hash}"
        ));
    }
    for (letter, expected) in FROZEN_WHOLE_CORPUS_LOGICAL_RUNS {
        let run = validated
            .runs
            .iter()
            .find(|run| run.configuration["run_letter"] == letter)
            .ok_or_else(|| format!("V3 production ingestion: missing frozen run {letter}"))?;
        if run.logical_run_sha256 != expected {
            return Err(format!(
                "V3 production ingestion: frozen run {letter} logical hash expected {expected}, actual {}",
                run.logical_run_sha256
            ));
        }
    }
    Ok(())
}

fn validate_quantization_inputs(validated: &ValidatedCollection) -> Result<(), String> {
    if validated.dimension == 0 || validated.dimension.saturating_mul(16_384) > i32::MAX as usize {
        return Err(format!(
            "V3 production ingestion: dimension {} cannot use the frozen signed-I32 I8 accumulator",
            validated.dimension
        ));
    }
    for (identity, values) in validated
        .corpus_embeddings
        .iter()
        .map(|row| (format!("{}/{}", row.record_id, row.chunk_key), &row.values))
        .chain(
            validated
                .query_embeddings
                .iter()
                .map(|row| (row.query_id.clone(), &row.values)),
        )
    {
        if values.len() != validated.dimension || values.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "V3 production ingestion: embedding '{identity}' is not a finite {}-dimension quantization input",
                validated.dimension
            ));
        }
        let max_abs = values
            .iter()
            .map(|value| value.abs())
            .fold(0.0_f32, f32::max);
        if !max_abs.is_finite() {
            return Err(format!(
                "V3 production ingestion: embedding '{identity}' has a non-finite I8 scale input"
            ));
        }
    }
    Ok(())
}

fn convert_record(source: &V3Record) -> Result<ProductionRecordInput, String> {
    let record_id = RecordId::new(source.record_id.clone())
        .map_err(|error| format!("V3 production ingestion: record ID: {error}"))?;
    let fields = source
        .fields
        .iter()
        .map(|(name, value)| {
            Ok((
                FieldName::new(name.clone())
                    .map_err(|error| format!("V3 production ingestion: field '{name}': {error}"))?,
                convert_record_value(
                    value,
                    &format!("record '{}'.fields.{name}", source.record_id),
                )?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let inherited_metadata = convert_metadata(&source.metadata, &source.record_id)?;
    let mut seen = BTreeSet::new();
    let chunks = source
        .chunks
        .iter()
        .map(|chunk| {
            let identity = (source.record_id.as_str(), chunk.chunk_key.as_str());
            if !seen.insert(identity) {
                return Err(format!(
                    "V3 production ingestion: duplicate chunk identity {}/{}",
                    identity.0, identity.1
                ));
            }
            Ok(CorpusChunkInput {
                key: ChunkKey::new(chunk.chunk_key.clone()).map_err(|error| {
                    format!(
                        "V3 production ingestion: chunk key {}/{}: {error}",
                        identity.0, identity.1
                    )
                })?,
                text: chunk.text.clone(),
                metadata: convert_metadata(
                    &chunk.metadata,
                    &format!("{}/{}", identity.0, identity.1),
                )?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProductionRecordInput {
        record: Record {
            id: record_id,
            record_type: RecordType::new(source.record_type.clone()).map_err(|error| {
                format!(
                    "V3 production ingestion: record '{}' type: {error}",
                    source.record_id
                )
            })?,
            fields,
            content: source.content.clone(),
        },
        inherited_metadata,
        chunks,
    })
}

fn convert_query(
    query: &Query,
    embeddings: &BTreeMap<&str, &[f32]>,
    dimension: usize,
) -> Result<ProductionQueryInput, String> {
    let embedding = embeddings.get(query.query_id.as_str()).ok_or_else(|| {
        format!(
            "V3 production ingestion: missing retrieval embedding for query '{}'",
            query.query_id
        )
    })?;
    if embedding.len() != dimension || embedding.iter().any(|value| !value.is_finite()) {
        return Err(format!(
            "V3 production ingestion: query '{}' embedding is not finite dimension {dimension}",
            query.query_id
        ));
    }
    Ok(ProductionQueryInput {
        query_id: query.query_id.clone(),
        text: query.text.clone(),
        embedding: embedding.to_vec(),
        filter: query
            .metadata_filter
            .as_ref()
            .map(convert_filter)
            .transpose()?,
    })
}

fn convert_record_value(value: &Value, path: &str) -> Result<RecordValue, String> {
    let tag = value["type"]
        .as_str()
        .ok_or_else(|| format!("V3 production ingestion: {path}.type is missing"))?;
    match tag {
        "null" => Ok(RecordValue::Null),
        "boolean" => value["value"]
            .as_bool()
            .map(RecordValue::Bool)
            .ok_or_else(|| format!("V3 production ingestion: {path} boolean value is invalid")),
        "integer" => value["value"]
            .as_i64()
            .map(RecordValue::I64)
            .ok_or_else(|| format!("V3 production ingestion: {path} integer value is invalid")),
        "float" => value["value"]
            .as_f64()
            .filter(|value| value.is_finite())
            .map(RecordValue::F64)
            .ok_or_else(|| format!("V3 production ingestion: {path} float value is invalid")),
        "string" => value["value"]
            .as_str()
            .map(|value| RecordValue::String(value.to_owned()))
            .ok_or_else(|| format!("V3 production ingestion: {path} string value is invalid")),
        "list" => value["value"]
            .as_array()
            .ok_or_else(|| format!("V3 production ingestion: {path} list value is invalid"))?
            .iter()
            .enumerate()
            .map(|(index, value)| convert_record_value(value, &format!("{path}[{index}]")))
            .collect::<Result<Vec<_>, _>>()
            .map(RecordValue::List),
        "object" => value["value"]
            .as_object()
            .ok_or_else(|| format!("V3 production ingestion: {path} object value is invalid"))?
            .iter()
            .map(|(field, value)| {
                Ok((
                    FieldName::new(field.clone()).map_err(|error| {
                        format!("V3 production ingestion: {path}.{field}: {error}")
                    })?,
                    convert_record_value(value, &format!("{path}.{field}"))?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(RecordValue::Map),
        _ => Err(format!(
            "V3 production ingestion: {path} has unsupported record value type '{tag}'"
        )),
    }
}

fn convert_metadata(values: &BTreeMap<String, Value>, owner: &str) -> Result<Metadata, String> {
    values
        .iter()
        .map(|(field, value)| {
            Ok((
                field.clone(),
                convert_metadata_value(value, &format!("{owner}.metadata.{field}"))?,
            ))
        })
        .collect()
}

fn convert_metadata_value(value: &Value, path: &str) -> Result<MetadataValue, String> {
    let tag = value["type"]
        .as_str()
        .ok_or_else(|| format!("V3 production ingestion: {path}.type is missing"))?;
    match tag {
        "string" => value["value"]
            .as_str()
            .map(|value| MetadataValue::String(value.to_owned())),
        "integer" => value["value"].as_i64().map(MetadataValue::Integer),
        "float" => value["value"]
            .as_f64()
            .filter(|value| value.is_finite())
            .map(MetadataValue::Float),
        "boolean" => value["value"].as_bool().map(MetadataValue::Boolean),
        "timestamp_millis" => value["value"].as_i64().map(MetadataValue::TimestampMillis),
        _ => None,
    }
    .ok_or_else(|| format!("V3 production ingestion: {path} has invalid metadata type '{tag}'"))
}

pub(super) fn convert_filter(value: &Value) -> Result<Filter, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "V3 production ingestion: filter is not an object".to_owned())?;
    let op = object["op"]
        .as_str()
        .ok_or_else(|| "V3 production ingestion: filter op is missing".to_owned())?;
    let field = || {
        object["field"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("V3 production ingestion: filter '{op}' field is missing"))
    };
    match op {
        "equals" => Ok(Filter::Equals {
            field: field()?,
            value: convert_metadata_value(&object["value"], "filter.value")?,
        }),
        "not_equals" => Ok(Filter::NotEquals {
            field: field()?,
            value: convert_metadata_value(&object["value"], "filter.value")?,
        }),
        "in" => Ok(Filter::In {
            field: field()?,
            values: object["values"]
                .as_array()
                .ok_or_else(|| "V3 production ingestion: filter.in values are missing".to_owned())?
                .iter()
                .map(|value| convert_metadata_value(value, "filter.values[]"))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        "range" => Ok(Filter::Range {
            field: field()?,
            lower: (!object["lower"].is_null())
                .then(|| convert_metadata_value(&object["lower"], "filter.lower"))
                .transpose()?,
            upper: (!object["upper"].is_null())
                .then(|| convert_metadata_value(&object["upper"], "filter.upper"))
                .transpose()?,
        }),
        "exists" => Ok(Filter::Exists { field: field()? }),
        "all" | "any" => {
            let children = object["children"]
                .as_array()
                .ok_or_else(|| {
                    format!("V3 production ingestion: filter '{op}' children are missing")
                })?
                .iter()
                .map(convert_filter)
                .collect::<Result<Vec<_>, _>>()?;
            if op == "all" {
                Ok(Filter::All(children))
            } else {
                Ok(Filter::Any(children))
            }
        }
        _ => Err(format!(
            "V3 production ingestion: unsupported filter op '{op}'"
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::quality::v3_validation::validate;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    #[test]
    fn maps_frozen_v3_records_queries_and_filters_to_production_inputs() {
        let validated = validate(&fixture_root()).unwrap();
        let inputs = V3ProductionInputs::from_validated(&validated).unwrap();

        assert_eq!(inputs.corpus_id.as_str(), "vectorkit-v3-synthetic-corpus");
        assert_eq!(inputs.dimension, 3);
        assert_eq!(inputs.records.len(), 7);
        assert_eq!(
            inputs
                .records
                .iter()
                .map(|record| record.chunks.len())
                .sum::<usize>(),
            8
        );
        assert_eq!(
            inputs
                .queries
                .iter()
                .map(|query| query.query_id.as_str())
                .collect::<Vec<_>>(),
            ["qa", "qb", "qd", "qf", "qg", "qh", "qi"]
        );
        assert!(inputs.queries[0].filter.is_none());
        assert!(inputs.queries[1].filter.is_some());
        assert_eq!(inputs.queries[1].text, "phone battery");
        assert_eq!(inputs.queries[0].embedding, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn ingests_f32_and_i8_through_the_production_database() {
        let validated = validate(&fixture_root()).unwrap();
        let inputs = V3ProductionInputs::from_validated(&validated).unwrap();

        for encoding in [VectorEncoding::F32, VectorEncoding::I8ScalarQuantized] {
            let database = inputs.build_database(encoding).unwrap();
            assert_eq!(database.corpus().active_chunk_count(), 8);
            assert_eq!(database.corpus().record_store().len(), 7);
            assert_eq!(database.corpus().generation().get(), 7);
            let identities = database
                .corpus()
                .chunk_identities()
                .map(|(identity, chunk_id)| {
                    (
                        identity.record_id.as_str().to_owned(),
                        identity.chunk_key.as_str().to_owned(),
                        chunk_id,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                identities,
                [
                    ("alpha".to_owned(), "details".to_owned(), 0),
                    ("alpha".to_owned(), "summary".to_owned(), 1),
                    ("beta".to_owned(), "summary".to_owned(), 2),
                    ("gamma".to_owned(), "summary".to_owned(), 3),
                    ("mobile".to_owned(), "summary".to_owned(), 4),
                    ("phone".to_owned(), "summary".to_owned(), 5),
                    ("shared-east".to_owned(), "summary".to_owned(), 6),
                    ("shared-west".to_owned(), "summary".to_owned(), 7),
                ]
            );
        }
    }

    #[test]
    fn rejects_non_finite_i8_quantization_inputs_before_ingestion() {
        let mut validated = validate(&fixture_root()).unwrap();
        validated.query_embeddings[0].values[0] = f32::NAN;

        let error = V3ProductionInputs::from_validated(&validated).unwrap_err();
        assert!(error.contains("not a finite 3-dimension quantization input"));
    }

    #[test]
    fn rejects_unsupported_metadata_types_during_mapping() {
        let error = convert_metadata_value(
            &serde_json::json!({"type":"list","value":[]}),
            "record.metadata.bad",
        )
        .unwrap_err();
        assert!(error.contains("invalid metadata type 'list'"));
    }
}
