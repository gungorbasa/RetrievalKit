use crate::{
    Bm25Config, ChunkKey, CorpusId, Document, HybridRetrievalConfiguration, IndexConfig, Metadata,
    Record, RecordChunkInput, RecordId, RecordType, Result, RetrievalConfiguration,
    RetrievalDatabase, RetrievalKitError, VectorEncoding, VectorMetric,
};

const DOCUMENT_RECORD_TYPE: &str = "Document";

/// Rust-owned progressive builder used by every language binding.
///
/// The first non-empty embedding fixes the database dimension. Records without
/// searchable documents may be queued before that point so graph-enabled
/// builders do not need wrapper-side pending state.
#[derive(Debug)]
pub struct RetrievalDatabaseBuilder {
    corpus_id: CorpusId,
    metric: VectorMetric,
    encoding: VectorEncoding,
    bm25: Bm25Config,
    database: Option<RetrievalDatabase>,
    pending: Vec<PendingRecord>,
}

#[derive(Debug, Clone)]
struct PendingRecord {
    record: Record,
    inherited_metadata: Metadata,
    chunks: Vec<RecordChunkInput>,
}

impl RetrievalDatabaseBuilder {
    pub fn new(corpus_id: CorpusId, metric: VectorMetric, encoding: VectorEncoding) -> Self {
        Self {
            corpus_id,
            metric,
            encoding,
            bm25: Bm25Config::default(),
            database: None,
            pending: Vec::new(),
        }
    }

    pub fn try_with_bm25_config(mut self, configuration: Bm25Config) -> Result<Self> {
        configuration.validate()?;
        self.bm25 = configuration;
        Ok(self)
    }

    /// Adds one public document and derives its canonical record/chunk model in
    /// Rust so wrappers never need to manufacture hidden identities.
    pub fn upsert_document(&mut self, document: Document, embedding: Vec<f32>) -> Result<Vec<u64>> {
        let record_id = RecordId::new(document.id)?;
        let chunk_key = ChunkKey::new(record_id.as_str())?;
        let record = Record {
            id: record_id,
            record_type: RecordType::new(DOCUMENT_RECORD_TYPE)?,
            fields: Default::default(),
            content: Some(document.text.clone()),
        };
        self.upsert_record(
            record,
            document.metadata,
            vec![RecordChunkInput {
                key: chunk_key,
                text: document.text,
                embedding,
                metadata: Metadata::new(),
            }],
        )
    }

    /// Adds one canonical record. Empty chunk lists are retained until an
    /// embedding fixes the retrieval configuration.
    pub fn upsert_record(
        &mut self,
        record: Record,
        inherited_metadata: Metadata,
        chunks: Vec<RecordChunkInput>,
    ) -> Result<Vec<u64>> {
        record.validate()?;
        if chunks.is_empty() && self.database.is_none() {
            self.pending.push(PendingRecord {
                record,
                inherited_metadata,
                chunks,
            });
            return Ok(Vec::new());
        }

        let Some(first) = chunks.first() else {
            let Some(database) = self.database.as_mut() else {
                return Err(RetrievalKitError::MissingEmbedding {
                    message: "at least one searchable document is required before building a retrieval database"
                        .to_owned(),
                });
            };
            return database.upsert_record(record, inherited_metadata, chunks);
        };
        let dimension = first.embedding.len();
        if dimension == 0 {
            return Err(RetrievalKitError::MissingEmbedding {
                message:
                    "embedding must contain at least one value; pass the vector produced by your embedding model"
                        .to_owned(),
            });
        }

        if let Some(database) = self.database.as_mut() {
            let expected = database.retrieval().dimension();
            if dimension != expected {
                return Err(RetrievalKitError::InvalidDimension {
                    expected,
                    actual: dimension,
                });
            }
            return database.upsert_record(record, inherited_metadata, chunks);
        }

        let vector = IndexConfig::new(dimension, self.metric).with_vector_encoding(self.encoding);
        let configuration = RetrievalConfiguration::semantic(vector).with_hybrid_configuration(
            HybridRetrievalConfiguration {
                bm25: self.bm25.clone(),
            },
        );
        let mut database = RetrievalDatabase::new(configuration, self.corpus_id.clone())?;
        for input in &self.pending {
            database.upsert_record(
                input.record.clone(),
                input.inherited_metadata.clone(),
                input.chunks.clone(),
            )?;
        }
        let chunk_ids = database.upsert_record(record, inherited_metadata, chunks)?;
        self.pending.clear();
        self.database = Some(database);
        Ok(chunk_ids)
    }

    pub fn build(self) -> Result<RetrievalDatabase> {
        self.database
            .ok_or_else(|| RetrievalKitError::MissingEmbedding {
                message: "at least one searchable document is required before building a retrieval database"
                    .to_owned(),
            })
    }

    pub fn dimension(&self) -> Option<usize> {
        self.database
            .as_ref()
            .map(|database| database.retrieval().dimension())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(id: &str, text: &str) -> Document {
        Document {
            id: id.to_owned(),
            text: text.to_owned(),
            metadata: Metadata::new(),
        }
    }

    #[test]
    fn infers_dimension_and_derives_document_identity() {
        let mut builder = RetrievalDatabaseBuilder::new(
            CorpusId::new("documents").unwrap(),
            VectorMetric::DotProduct,
            VectorEncoding::F32,
        );
        builder
            .upsert_document(document("swift", "native search"), vec![1.0, 0.0])
            .unwrap();
        assert_eq!(builder.dimension(), Some(2));

        let database = builder.build().unwrap();
        let hit = database
            .semantic_search(&crate::SearchQuery::new(vec![1.0, 0.0], 1))
            .unwrap()
            .remove(0);
        assert_eq!(hit.document_id, "swift");
        let identity = database
            .corpus()
            .chunk_identity(hit.chunk_id)
            .expect("stable identity");
        assert_eq!(identity.record_id.as_str(), "swift");
        assert_eq!(identity.chunk_key.as_str(), "swift");
    }

    #[test]
    fn rejects_empty_embeddings_and_dimension_drift() {
        let mut builder = RetrievalDatabaseBuilder::new(
            CorpusId::new("dimensions").unwrap(),
            VectorMetric::Cosine,
            VectorEncoding::F32,
        );
        assert!(matches!(
            builder.upsert_document(document("empty", "empty"), vec![]),
            Err(RetrievalKitError::MissingEmbedding { .. })
        ));
        builder
            .upsert_document(document("first", "first"), vec![1.0, 0.0])
            .unwrap();
        assert_eq!(
            builder
                .upsert_document(document("second", "second"), vec![1.0])
                .unwrap_err(),
            RetrievalKitError::InvalidDimension {
                expected: 2,
                actual: 1
            }
        );
    }

    #[test]
    fn queues_graph_only_records_until_dimension_is_known() {
        let mut builder = RetrievalDatabaseBuilder::new(
            CorpusId::new("pending").unwrap(),
            VectorMetric::DotProduct,
            VectorEncoding::F32,
        );
        builder
            .upsert_record(
                Record {
                    id: RecordId::new("graph-only").unwrap(),
                    record_type: RecordType::new("Topic").unwrap(),
                    fields: Default::default(),
                    content: None,
                },
                Metadata::new(),
                vec![],
            )
            .unwrap();
        builder
            .upsert_document(document("searchable", "searchable"), vec![1.0, 0.0])
            .unwrap();

        let database = builder.build().unwrap();
        assert!(database
            .corpus()
            .record(&RecordId::new("graph-only").unwrap())
            .is_some());
    }

    #[test]
    fn failed_first_upsert_does_not_fix_the_builder_dimension() {
        let mut builder = RetrievalDatabaseBuilder::new(
            CorpusId::new("transactional").unwrap(),
            VectorMetric::DotProduct,
            VectorEncoding::F32,
        );
        let record = Record {
            id: RecordId::new("invalid").unwrap(),
            record_type: RecordType::new("Topic").unwrap(),
            fields: Default::default(),
            content: None,
        };
        let error = builder
            .upsert_record(
                record,
                Metadata::new(),
                vec![
                    RecordChunkInput {
                        key: ChunkKey::new("first").unwrap(),
                        text: "first".to_owned(),
                        embedding: vec![1.0, 0.0],
                        metadata: Metadata::new(),
                    },
                    RecordChunkInput {
                        key: ChunkKey::new("second").unwrap(),
                        text: "second".to_owned(),
                        embedding: vec![1.0],
                        metadata: Metadata::new(),
                    },
                ],
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RetrievalKitError::InvalidDimension {
                expected: 2,
                actual: 1
            }
        ));
        assert_eq!(builder.dimension(), None);

        builder
            .upsert_document(document("valid", "valid"), vec![1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(builder.dimension(), Some(3));
    }
}
