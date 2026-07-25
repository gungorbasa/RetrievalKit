use retrievalkit_core::{
    ChunkKey, CorpusChunkInput, CorpusId, CorpusIndex, EmbeddedDocument, Metadata, Record,
    RecordChunkInput, RecordInput, RetrievalDatabaseBuilder, RetrievalKitError, VectorEncoding,
    VectorMetric,
};

use crate::{GraphDatabase, GraphRetrievalDatabase, GraphSchema, Result};

/// Rust-owned graph-only builder. Record content is projected into the
/// canonical corpus in Rust rather than by a language wrapper.
#[derive(Debug)]
pub struct GraphDatabaseBuilder {
    corpus: CorpusIndex,
    schema: GraphSchema,
}

impl GraphDatabaseBuilder {
    pub fn new(corpus_id: CorpusId, schema: GraphSchema) -> Self {
        Self {
            corpus: CorpusIndex::new(corpus_id),
            schema,
        }
    }

    pub fn upsert_record(
        &mut self,
        record: Record,
        projected_metadata: Metadata,
    ) -> Result<Vec<u64>> {
        let chunks = match record.content.as_ref() {
            Some(content) => vec![CorpusChunkInput {
                key: ChunkKey::new(record.id.as_str())?,
                text: content.clone(),
                metadata: Metadata::new(),
            }],
            None => Vec::new(),
        };
        self.upsert_input(RecordInput {
            record,
            metadata: projected_metadata,
            chunks,
        })
    }

    pub fn upsert_input(&mut self, input: RecordInput) -> Result<Vec<u64>> {
        self.corpus.upsert(input).map_err(Into::into)
    }

    pub fn build(self) -> Result<GraphDatabase> {
        GraphDatabase::build(self.corpus, self.schema)
    }
}

/// Rust-owned progressive graph + retrieval builder shared by bindings.
#[derive(Debug)]
pub struct GraphRetrievalDatabaseBuilder {
    retrieval: RetrievalDatabaseBuilder,
    schema: GraphSchema,
}

impl GraphRetrievalDatabaseBuilder {
    pub fn new(
        corpus_id: CorpusId,
        schema: GraphSchema,
        metric: VectorMetric,
        encoding: VectorEncoding,
    ) -> Self {
        Self {
            retrieval: RetrievalDatabaseBuilder::new(corpus_id, metric, encoding),
            schema,
        }
    }

    pub fn upsert_record(
        &mut self,
        record: Record,
        projected_metadata: Metadata,
    ) -> Result<Vec<u64>> {
        self.retrieval
            .upsert_record(record, projected_metadata, Vec::new())
            .map_err(Into::into)
    }

    pub fn upsert_record_with_embedding(
        &mut self,
        record: Record,
        projected_metadata: Metadata,
        embedding: Vec<f32>,
    ) -> Result<Vec<u64>> {
        let Some(content) = record.content.clone() else {
            return Err(RetrievalKitError::MissingEmbedding {
                message: format!(
                    "record '{}' has no content to pair with the embedding",
                    record.id.as_str()
                ),
            }
            .into());
        };
        let key = ChunkKey::new(record.id.as_str())?;
        self.retrieval
            .upsert_record(
                record,
                projected_metadata,
                vec![RecordChunkInput {
                    key,
                    text: content,
                    embedding,
                    metadata: Metadata::new(),
                }],
            )
            .map_err(Into::into)
    }

    pub fn upsert_record_documents(
        &mut self,
        record: Record,
        projected_metadata: Metadata,
        documents: Vec<EmbeddedDocument>,
    ) -> Result<Vec<u64>> {
        let chunks = documents
            .into_iter()
            .map(|embedded| {
                Ok(RecordChunkInput {
                    key: ChunkKey::new(embedded.document.id)?,
                    text: embedded.document.text,
                    embedding: embedded.embedding,
                    metadata: embedded.document.metadata,
                })
            })
            .collect::<retrievalkit_core::Result<Vec<_>>>()?;
        self.retrieval
            .upsert_record(record, projected_metadata, chunks)
            .map_err(Into::into)
    }

    pub fn upsert_record_chunks(
        &mut self,
        record: Record,
        projected_metadata: Metadata,
        chunks: Vec<RecordChunkInput>,
    ) -> Result<Vec<u64>> {
        self.retrieval
            .upsert_record(record, projected_metadata, chunks)
            .map_err(Into::into)
    }

    pub fn build(self) -> Result<GraphRetrievalDatabase> {
        GraphRetrievalDatabase::build(self.retrieval.build()?, self.schema)
    }

    pub fn dimension(&self) -> Option<usize> {
        self.retrieval.dimension()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use retrievalkit_core::{FieldName, RecordId, RecordType, RecordValue};

    use super::*;
    use crate::{GraphQuery, NodeType, RecordNodeSchema, Seed};

    fn schema() -> GraphSchema {
        GraphSchema::new(vec![RecordNodeSchema {
            record_type: RecordType::new("Topic").unwrap(),
            node_type: NodeType::new("Topic").unwrap(),
            queryable_fields: vec![],
        }])
    }

    fn record(id: &str, content: Option<&str>) -> Record {
        Record {
            id: RecordId::new(id).unwrap(),
            record_type: RecordType::new("Topic").unwrap(),
            fields: BTreeMap::from([(
                FieldName::new("title").unwrap(),
                RecordValue::String(id.to_owned()),
            )]),
            content: content.map(str::to_owned),
        }
    }

    #[test]
    fn graph_only_builder_projects_record_content_in_rust() {
        let mut builder = GraphDatabaseBuilder::new(CorpusId::new("graph-only").unwrap(), schema());
        builder
            .upsert_record(record("swift", Some("native graph")), Metadata::new())
            .unwrap();
        let database = builder.build().unwrap();
        assert_eq!(database.corpus().active_chunk_count(), 1);
    }

    #[test]
    fn combined_builder_queues_graph_only_records_and_infers_dimension() {
        let mut builder = GraphRetrievalDatabaseBuilder::new(
            CorpusId::new("combined").unwrap(),
            schema(),
            VectorMetric::DotProduct,
            VectorEncoding::F32,
        );
        builder
            .upsert_record(record("graph-only", None), Metadata::new())
            .unwrap();
        builder
            .upsert_record_with_embedding(
                record("searchable", Some("native retrieval")),
                Metadata::new(),
                vec![1.0, 0.0],
            )
            .unwrap();
        assert_eq!(builder.dimension(), Some(2));

        let database = builder.build().unwrap();
        let selection = database
            .graph_query(
                &GraphQuery::new(Seed::NodeIds(vec![crate::NodeId::record(
                    NodeType::new("Topic").unwrap(),
                    RecordId::new("graph-only").unwrap(),
                )])),
                None,
            )
            .unwrap();
        assert_eq!(selection.matches.len(), 1);
        assert_eq!(database.corpus().active_chunk_count(), 1);
    }
}
