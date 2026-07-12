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

/// Optional hybrid retrieval state derived alongside semantic vectors.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HybridRetrievalConfiguration {
    pub bm25: Bm25Config,
}

/// Semantic retrieval configuration plus optional derived capabilities.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalConfiguration {
    pub semantic: IndexConfig,
    pub hybrid: Option<HybridRetrievalConfiguration>,
}

impl RetrievalConfiguration {
    pub fn semantic(vector: IndexConfig) -> Self {
        Self {
            semantic: vector,
            hybrid: None,
        }
    }

    pub fn with_hybrid(mut self) -> Self {
        self.hybrid = Some(HybridRetrievalConfiguration::default());
        self
    }

    pub fn with_hybrid_configuration(
        mut self,
        configuration: HybridRetrievalConfiguration,
    ) -> Self {
        self.hybrid = Some(configuration);
        self
    }

    pub fn mode(&self) -> RetrievalMode {
        if self.hybrid.is_some() {
            RetrievalMode::Hybrid
        } else {
            RetrievalMode::Semantic
        }
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
        let mode = configuration.mode();
        let vector = configuration.vector();
        let bm25 = configuration
            .hybrid
            .as_ref()
            .map(|hybrid| Bm25Index::new(hybrid.bm25.clone()));
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
            crate::error::VectorKitError::RetrievalCapabilityUnavailable {
                capability: "hybrid",
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_mode_does_not_construct_bm25_state() {
        let retrieval = RetrievalIndex::new(RetrievalConfiguration::semantic(IndexConfig::new(
            384,
            VectorMetric::Cosine,
        )))
        .unwrap();

        assert_eq!(retrieval.mode(), RetrievalMode::Semantic);
        assert!(!retrieval.has_bm25());
    }

    #[test]
    fn hybrid_mode_constructs_bm25_state() {
        let retrieval = RetrievalIndex::new(
            RetrievalConfiguration::semantic(IndexConfig::new(384, VectorMetric::Cosine))
                .with_hybrid(),
        )
        .unwrap();

        assert_eq!(retrieval.mode(), RetrievalMode::Hybrid);
        assert!(retrieval.has_bm25());
    }
}
