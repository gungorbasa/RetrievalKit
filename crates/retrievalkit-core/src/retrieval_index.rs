use crate::bm25::{Bm25Config, Bm25Index};
use crate::error::Result;
use crate::metadata_index::MetadataFilterIndex;
use crate::scoring::EncodedVectorStore;
use crate::types::{IndexConfig, VectorEncoding, VectorMetric};
use serde::{Deserialize, Serialize};

/// The derived retrieval capability enabled for a database generation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalMode {
    Semantic,
    #[default]
    Hybrid,
}

/// Hybrid retrieval state derived alongside semantic vectors.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HybridRetrievalConfiguration {
    pub bm25: Bm25Config,
}

/// Exact-vector and BM25 retrieval configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalConfiguration {
    pub semantic: IndexConfig,
    pub hybrid: HybridRetrievalConfiguration,
}

impl RetrievalConfiguration {
    pub fn semantic(vector: IndexConfig) -> Self {
        Self {
            semantic: vector,
            hybrid: HybridRetrievalConfiguration::default(),
        }
    }

    pub fn with_hybrid_configuration(
        mut self,
        configuration: HybridRetrievalConfiguration,
    ) -> Self {
        self.hybrid = configuration;
        self
    }

    pub fn mode(&self) -> RetrievalMode {
        RetrievalMode::Hybrid
    }

    pub fn vector(&self) -> &IndexConfig {
        &self.semantic
    }
}

/// Vector, lexical, and filter acceleration derived from one `CorpusIndex`.
///
/// The corpus remains the payload owner. This type stores only rebuildable
/// retrieval state whose row offsets are validated against that corpus.
#[derive(Debug, Clone)]
pub struct RetrievalIndex {
    pub(crate) mode: RetrievalMode,
    pub(crate) dimension: usize,
    pub(crate) metric: VectorMetric,
    pub(crate) vector_encoding: VectorEncoding,
    pub(crate) encoded_vectors: EncodedVectorStore,
    pub(crate) metadata_filter_index: MetadataFilterIndex,
    pub(crate) bm25: Option<Bm25Index>,
}

impl RetrievalIndex {
    pub fn new(configuration: RetrievalConfiguration) -> Result<Self> {
        configuration.hybrid.bm25.validate()?;
        let mode = configuration.mode();
        let vector = configuration.vector();
        let bm25 = Some(Bm25Index::new(configuration.hybrid.bm25.clone()));
        Ok(Self {
            mode,
            dimension: vector.dimension,
            metric: vector.metric,
            vector_encoding: vector.vector_encoding,
            encoded_vectors: EncodedVectorStore::new(vector.vector_encoding)?,
            metadata_filter_index: MetadataFilterIndex::default(),
            bm25,
        })
    }

    pub fn mode(&self) -> RetrievalMode {
        self.mode
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn metric(&self) -> VectorMetric {
        self.metric
    }

    pub fn vector_encoding(&self) -> VectorEncoding {
        self.vector_encoding
    }

    pub fn has_bm25(&self) -> bool {
        self.bm25.is_some()
    }

    pub(crate) fn require_bm25(&self) -> Result<&Bm25Index> {
        self.bm25.as_ref().ok_or(
            crate::error::RetrievalKitError::RetrievalCapabilityUnavailable {
                capability: "hybrid",
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_configuration_always_constructs_bm25_state() {
        let retrieval = RetrievalIndex::new(RetrievalConfiguration::semantic(IndexConfig::new(
            384,
            VectorMetric::Cosine,
        )))
        .unwrap();

        assert_eq!(retrieval.mode(), RetrievalMode::Hybrid);
        assert!(retrieval.has_bm25());
    }
}
