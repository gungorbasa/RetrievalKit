use std::collections::BTreeMap;

use crate::error::{Result, VectorKitError};
use crate::filter::Filter;
use crate::metadata::Metadata;
use crate::types::{
    Chunk, ChunkId, ChunkInput, Document, SearchHit, SearchQuery, SearchTrace, VectorMetric,
};

#[derive(Debug, Clone)]
pub struct ExactVectorIndex {
    dimension: usize,
    metric: VectorMetric,
    chunks: Vec<Chunk>,
    next_chunk_id: ChunkId,
    document_versions: BTreeMap<String, u64>,
}

impl ExactVectorIndex {
    pub fn new(dimension: usize, metric: VectorMetric) -> Self {
        Self {
            dimension,
            metric,
            chunks: Vec::new(),
            next_chunk_id: 0,
            document_versions: BTreeMap::new(),
        }
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn metric(&self) -> VectorMetric {
        self.metric
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn add_chunk(&mut self, chunk: Chunk) -> Result<()> {
        self.validate_dimension(chunk.embedding.len())?;
        self.next_chunk_id = self.next_chunk_id.max(chunk.chunk_id.saturating_add(1));
        self.document_versions
            .entry(chunk.document_id.clone())
            .and_modify(|version| *version = (*version).max(chunk.version))
            .or_insert(chunk.version);
        self.chunks.push(chunk);
        Ok(())
    }

    pub fn upsert_document(
        &mut self,
        document: Document,
        chunk_inputs: Vec<ChunkInput>,
    ) -> Result<Vec<ChunkId>> {
        for chunk in &chunk_inputs {
            self.validate_dimension(chunk.embedding.len())?;
        }

        let version = self
            .document_versions
            .get(&document.id)
            .copied()
            .unwrap_or(0)
            + 1;

        for chunk in &mut self.chunks {
            if chunk.document_id == document.id {
                chunk.deleted = true;
            }
        }

        let mut chunk_ids = Vec::with_capacity(chunk_inputs.len());
        for chunk_input in chunk_inputs {
            let chunk_id = self.allocate_chunk_id();
            chunk_ids.push(chunk_id);
            self.chunks.push(Chunk {
                chunk_id,
                document_id: document.id.clone(),
                text: chunk_input.text,
                embedding: chunk_input.embedding,
                metadata: merge_metadata(&document.metadata, chunk_input.metadata),
                deleted: false,
                version,
            });
        }

        self.document_versions.insert(document.id, version);

        Ok(chunk_ids)
    }

    pub fn delete_document(&mut self, document_id: &str) -> usize {
        let mut deleted_count = 0;
        for chunk in &mut self.chunks {
            if chunk.document_id == document_id && !chunk.deleted {
                chunk.deleted = true;
                deleted_count += 1;
            }
        }
        deleted_count
    }

    pub fn chunk(&self, chunk_id: ChunkId) -> Option<&Chunk> {
        self.chunks.iter().find(|chunk| chunk.chunk_id == chunk_id)
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.validate_dimension(query.embedding.len())?;

        if query.top_k == 0 {
            return Ok(Vec::new());
        }

        let mut hits = Vec::new();
        for chunk in &self.chunks {
            if chunk.deleted {
                continue;
            }

            if !matches_filter(query.filter.as_ref(), chunk)? {
                continue;
            }

            let score = self.metric.score(&query.embedding, &chunk.embedding);
            hits.push(SearchHit {
                chunk_id: chunk.chunk_id,
                document_id: chunk.document_id.clone(),
                score,
                trace: SearchTrace {
                    vector_score: score,
                    keyword_score: None,
                    filter_matched: true,
                },
            });
        }

        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.chunk_id.cmp(&right.chunk_id))
        });
        hits.truncate(query.top_k);

        Ok(hits)
    }

    fn validate_dimension(&self, actual: usize) -> Result<()> {
        if actual == self.dimension {
            Ok(())
        } else {
            Err(VectorKitError::InvalidDimension {
                expected: self.dimension,
                actual,
            })
        }
    }

    fn allocate_chunk_id(&mut self) -> ChunkId {
        let chunk_id = self.next_chunk_id;
        self.next_chunk_id = self.next_chunk_id.saturating_add(1);
        chunk_id
    }
}

fn matches_filter(filter: Option<&Filter>, chunk: &Chunk) -> Result<bool> {
    match filter {
        Some(filter) => filter.matches(&chunk.metadata),
        None => Ok(true),
    }
}

fn merge_metadata(document_metadata: &Metadata, chunk_metadata: Metadata) -> Metadata {
    let mut metadata = document_metadata.clone();
    metadata.extend(chunk_metadata);
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{Metadata, MetadataValue};

    fn chunk(chunk_id: ChunkId, document_id: &str, embedding: Vec<f32>) -> Chunk {
        Chunk {
            chunk_id,
            document_id: document_id.to_owned(),
            text: format!("chunk {chunk_id}"),
            embedding,
            metadata: Metadata::new(),
            deleted: false,
            version: 1,
        }
    }

    fn document(document_id: &str) -> Document {
        Document {
            id: document_id.to_owned(),
            text: format!("document {document_id}"),
            metadata: Metadata::new(),
        }
    }

    fn chunk_input(text: &str, embedding: Vec<f32>) -> ChunkInput {
        ChunkInput {
            text: text.to_owned(),
            embedding,
            metadata: Metadata::new(),
        }
    }

    #[test]
    fn rejects_chunks_with_wrong_dimension() {
        let mut index = ExactVectorIndex::new(3, VectorMetric::DotProduct);
        let error = index
            .add_chunk(chunk(1, "doc-1", vec![1.0, 0.0]))
            .unwrap_err();

        assert_eq!(
            error,
            VectorKitError::InvalidDimension {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn rejects_queries_with_wrong_dimension() {
        let index = ExactVectorIndex::new(3, VectorMetric::DotProduct);
        let query = SearchQuery::new(vec![1.0, 0.0], 10);

        let error = index.search(&query).unwrap_err();

        assert_eq!(
            error,
            VectorKitError::InvalidDimension {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn exact_search_excludes_deleted_chunks() {
        let mut index = ExactVectorIndex::new(3, VectorMetric::DotProduct);
        index
            .add_chunk(chunk(1, "doc-1", vec![1.0, 0.0, 0.0]))
            .unwrap();

        let mut deleted = chunk(2, "doc-2", vec![2.0, 0.0, 0.0]);
        deleted.deleted = true;
        index.add_chunk(deleted).unwrap();

        let hits = index
            .search(&SearchQuery::new(vec![1.0, 0.0, 0.0], 10))
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
    }

    #[test]
    fn exact_search_is_deterministic_for_tied_scores() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .add_chunk(chunk(20, "doc-20", vec![1.0, 0.0]))
            .unwrap();
        index
            .add_chunk(chunk(10, "doc-10", vec![1.0, 0.0]))
            .unwrap();

        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    #[test]
    fn exact_search_applies_metadata_filters() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut notes_chunk = chunk(1, "doc-1", vec![1.0, 0.0]);
        notes_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index.add_chunk(notes_chunk).unwrap();

        let mut transcript_chunk = chunk(2, "doc-2", vec![2.0, 0.0]);
        transcript_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index.add_chunk(transcript_chunk).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("notes".to_owned()),
        });

        let hits = index.search(&query).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
    }

    #[test]
    fn upsert_document_assigns_internal_chunk_ids_and_version() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);

        let chunk_ids = index
            .upsert_document(
                document("doc-1"),
                vec![
                    chunk_input("first", vec![1.0, 0.0]),
                    chunk_input("second", vec![0.0, 1.0]),
                ],
            )
            .unwrap();

        assert_eq!(chunk_ids, vec![0, 1]);
        assert_eq!(index.chunk(0).unwrap().version, 1);
        assert_eq!(index.chunk(1).unwrap().version, 1);
    }

    #[test]
    fn upsert_document_marks_old_chunks_deleted() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let old_chunk_ids = index
            .upsert_document(document("doc-1"), vec![chunk_input("old", vec![1.0, 0.0])])
            .unwrap();

        let new_chunk_ids = index
            .upsert_document(document("doc-1"), vec![chunk_input("new", vec![2.0, 0.0])])
            .unwrap();

        assert_eq!(old_chunk_ids, vec![0]);
        assert_eq!(new_chunk_ids, vec![1]);
        assert!(index.chunk(0).unwrap().deleted);
        assert_eq!(index.chunk(1).unwrap().version, 2);

        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
    }

    #[test]
    fn delete_document_removes_active_chunks_from_search() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![
                    chunk_input("first", vec![1.0, 0.0]),
                    chunk_input("second", vec![0.0, 1.0]),
                ],
            )
            .unwrap();

        let deleted_count = index.delete_document("doc-1");
        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(deleted_count, 2);
        assert!(hits.is_empty());
    }

    #[test]
    fn upsert_document_validates_all_chunks_before_mutating_existing_document() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(document("doc-1"), vec![chunk_input("old", vec![1.0, 0.0])])
            .unwrap();

        let error = index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("invalid", vec![1.0, 0.0, 0.0])],
            )
            .unwrap_err();
        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(
            error,
            VectorKitError::InvalidDimension {
                expected: 2,
                actual: 3
            }
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 0);
        assert!(!index.chunk(0).unwrap().deleted);
    }

    #[test]
    fn upsert_document_merges_document_and_chunk_metadata_for_filters() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut document = document("doc-1");
        document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );

        let mut chunk = chunk_input("first", vec![1.0, 0.0]);
        chunk.metadata.insert(
            "section".to_owned(),
            MetadataValue::String("intro".to_owned()),
        );

        index.upsert_document(document, vec![chunk]).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::All(vec![
            Filter::Equals {
                field: "source".to_owned(),
                value: MetadataValue::String("notes".to_owned()),
            },
            Filter::Equals {
                field: "section".to_owned(),
                value: MetadataValue::String("intro".to_owned()),
            },
        ]));

        let hits = index.search(&query).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 0);
    }
}
