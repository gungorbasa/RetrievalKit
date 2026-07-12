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

/// Configuration selected before records are ingested.
#[derive(Debug, Clone, PartialEq)]
pub enum RetrievalConfiguration {
    Semantic {
        vector: IndexConfig,
    },
    Hybrid {
        vector: IndexConfig,
        bm25: Bm25Config,
    },
}

impl RetrievalConfiguration {
    pub fn semantic(vector: IndexConfig) -> Self {
        Self::Semantic { vector }
    }

    pub fn hybrid(vector: IndexConfig) -> Self {
        Self::Hybrid {
            vector,
            bm25: Bm25Config::default(),
        }
    }

    pub fn mode(&self) -> RetrievalMode {
        match self {
            Self::Semantic { .. } => RetrievalMode::Semantic,
            Self::Hybrid { .. } => RetrievalMode::Hybrid,
        }
    }

    pub fn vector(&self) -> &IndexConfig {
        match self {
            Self::Semantic { vector } | Self::Hybrid { vector, .. } => vector,
        }
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
        let bm25 = match &configuration {
            RetrievalConfiguration::Semantic { .. } => None,
            RetrievalConfiguration::Hybrid { bm25, .. } => Some(Bm25Index::new(bm25.clone())),
        };
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
        self.bm25
            .as_ref()
            .ok_or(crate::error::VectorKitError::RetrievalModeUnavailable {
                required: "hybrid",
                actual: "semantic",
            })
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
        let retrieval = RetrievalIndex::new(RetrievalConfiguration::hybrid(IndexConfig::new(
            384,
            VectorMetric::Cosine,
        )))
        .unwrap();

        assert_eq!(retrieval.mode(), RetrievalMode::Hybrid);
        assert!(retrieval.has_bm25());
    }
}
