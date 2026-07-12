use std::path::Path;

use crate::candidate_scope::CandidateScope;
use crate::corpus_index::CorpusIndex;
use crate::error::Result;
use crate::index::ExactVectorIndex;
use crate::metadata::Metadata;
use crate::record_store::{ChunkIdentity, CorpusId, Record, RecordId};
use crate::retrieval_index::{RetrievalConfiguration, RetrievalIndex, RetrievalMode};
use crate::types::{
    CompactionReport, HybridHit, HybridQuery, IndexFileSizeReport, RecordChunkInput, SearchHit,
    SearchQuery, StoredChunk,
};

/// A graph-neutral database with semantic or hybrid retrieval enabled.
#[derive(Debug, Clone)]
pub struct RetrievalDatabase {
    index: ExactVectorIndex,
}

impl RetrievalDatabase {
    pub fn new(configuration: RetrievalConfiguration, corpus_id: CorpusId) -> Result<Self> {
        Ok(Self {
            index: ExactVectorIndex::try_with_retrieval_configuration_in_corpus(
                configuration,
                corpus_id,
            )?,
        })
    }

    pub fn mode(&self) -> RetrievalMode {
        self.index.retrieval().mode()
    }

    pub fn corpus(&self) -> &CorpusIndex {
        self.index.corpus()
    }

    pub fn retrieval(&self) -> &RetrievalIndex {
        self.index.retrieval()
    }

    pub fn upsert_record(
        &mut self,
        record: Record,
        inherited_metadata: Metadata,
        chunks: Vec<RecordChunkInput>,
    ) -> Result<Vec<u64>> {
        self.index.upsert_record(record, inherited_metadata, chunks)
    }

    pub fn delete_record(&mut self, record_id: &RecordId) -> usize {
        self.index.delete_record(record_id)
    }

    pub fn compact(&mut self) -> Result<CompactionReport> {
        self.index.compact()
    }

    pub fn semantic_search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.index.search(query)
    }

    pub fn semantic_search_in_candidates(
        &self,
        query: &SearchQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<SearchHit>> {
        self.index.search_in_candidates(query, scope)
    }

    pub fn hybrid_search(&self, query: &HybridQuery) -> Result<Vec<HybridHit>> {
        self.index.hybrid_search(query)
    }

    pub fn hybrid_search_in_candidates(
        &self,
        query: &HybridQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<HybridHit>> {
        self.index.hybrid_search_in_candidates(query, scope)
    }

    pub fn candidate_scope(
        &self,
        chunk_ids: impl IntoIterator<Item = u64>,
    ) -> Result<CandidateScope> {
        self.index.candidate_scope(chunk_ids)
    }

    pub fn candidate_scope_for_identities(
        &self,
        identities: impl IntoIterator<Item = ChunkIdentity>,
    ) -> Result<CandidateScope> {
        self.index.candidate_scope_for_identities(identities)
    }

    pub fn chunk(&self, chunk_id: u64) -> Option<&StoredChunk> {
        self.index.chunk(chunk_id)
    }

    pub fn save_to_dir(&self, directory: impl AsRef<Path>) -> Result<IndexFileSizeReport> {
        self.index.save_to_dir(directory)
    }

    pub fn load_from_dir(directory: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            index: ExactVectorIndex::load_from_dir(directory)?,
        })
    }

    pub fn validate_dir(directory: impl AsRef<Path>) -> Result<()> {
        ExactVectorIndex::validate_dir(directory)
    }

    pub fn as_compatibility_index(&self) -> &ExactVectorIndex {
        &self.index
    }

    pub fn into_compatibility_index(self) -> ExactVectorIndex {
        self.index
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::metadata::Metadata;
    use crate::record_store::{ChunkKey, FieldName, RecordType, RecordValue};
    use crate::types::{IndexConfig, VectorMetric};
    use crate::VectorKitError;

    fn record() -> Record {
        Record {
            id: RecordId::new("rust").unwrap(),
            record_type: RecordType::new("Topic").unwrap(),
            fields: BTreeMap::from([(
                FieldName::new("title").unwrap(),
                RecordValue::String("Rust".to_owned()),
            )]),
            content: None,
        }
    }

    fn chunk() -> RecordChunkInput {
        RecordChunkInput {
            key: ChunkKey::new("summary").unwrap(),
            text: "native retrieval".to_owned(),
            embedding: vec![1.0, 0.0],
            metadata: Metadata::new(),
        }
    }

    #[test]
    fn semantic_database_has_no_bm25_and_rejects_hybrid_queries() {
        let mut database = RetrievalDatabase::new(
            RetrievalConfiguration::semantic(IndexConfig::new(2, VectorMetric::DotProduct)),
            CorpusId::new("semantic").unwrap(),
        )
        .unwrap();
        database
            .upsert_record(record(), Metadata::new(), vec![chunk()])
            .unwrap();

        assert!(!database.retrieval().has_bm25());
        assert_eq!(
            database
                .semantic_search(&SearchQuery::new(vec![1.0, 0.0], 1))
                .unwrap()
                .len(),
            1
        );
        assert!(matches!(
            database.hybrid_search(&HybridQuery::new("native", vec![1.0, 0.0], 1)),
            Err(VectorKitError::RetrievalModeUnavailable { .. })
        ));
    }

    #[test]
    fn hybrid_database_supports_semantic_and_hybrid_queries() {
        let mut database = RetrievalDatabase::new(
            RetrievalConfiguration::hybrid(IndexConfig::new(2, VectorMetric::DotProduct)),
            CorpusId::new("hybrid").unwrap(),
        )
        .unwrap();
        database
            .upsert_record(record(), Metadata::new(), vec![chunk()])
            .unwrap();

        assert!(database.retrieval().has_bm25());
        assert_eq!(
            database
                .semantic_search(&SearchQuery::new(vec![1.0, 0.0], 1))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            database
                .hybrid_search(&HybridQuery::new("native", vec![1.0, 0.0], 1))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn semantic_database_persists_without_bm25_payload() {
        let mut database = RetrievalDatabase::new(
            RetrievalConfiguration::semantic(IndexConfig::new(2, VectorMetric::DotProduct)),
            CorpusId::new("semantic-persistence").unwrap(),
        )
        .unwrap();
        database
            .upsert_record(record(), Metadata::new(), vec![chunk()])
            .unwrap();

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("vectorkit-semantic-{}-{nonce}", std::process::id()));
        let report = database.save_to_dir(&directory).unwrap();
        assert_eq!(report.bm25_bytes, 0);

        let loaded = RetrievalDatabase::load_from_dir(&directory).unwrap();
        assert_eq!(loaded.mode(), RetrievalMode::Semantic);
        assert!(!loaded.retrieval().has_bm25());
        assert_eq!(
            loaded
                .semantic_search(&SearchQuery::new(vec![1.0, 0.0], 1))
                .unwrap()
                .len(),
            1
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
