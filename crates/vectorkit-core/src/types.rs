use crate::filter::Filter;
use crate::metadata::Metadata;
use serde::{Deserialize, Serialize};

pub type ChunkId = u64;

/// Caller-owned document data.
///
/// The `id` must be stable across app launches and is used for update,
/// delete, and result grouping. VectorKit assigns internal `ChunkId` values,
/// but it does not generate document IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: String,
    pub text: String,
    pub metadata: Metadata,
}

/// Caller-provided retrievable unit with an internal numeric ID.
///
/// Chunks are the search result unit. Callers should usually provide
/// `ChunkInput` values through `ExactVectorIndex::upsert_document` and let the
/// index assign `chunk_id` values. The searchable index encodes `embedding`
/// into its configured vector store and does not retain this source vector in
/// hot chunk metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub chunk_id: ChunkId,
    pub document_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: Metadata,
    pub deleted: bool,
    pub version: u64,
}

/// Indexed chunk metadata retained by the searchable index.
///
/// Vector values are stored separately in the encoded vector store. This keeps
/// search metadata and display data separate from the hot vector layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredChunk {
    pub chunk_id: ChunkId,
    pub document_id: String,
    pub text: String,
    pub metadata: Metadata,
    pub deleted: bool,
    pub version: u64,
}

/// Caller-provided chunk data used when indexing or replacing a document.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkInput {
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: Metadata,
}

/// Exact vector search request.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    pub embedding: Vec<f32>,
    pub top_k: usize,
    pub filter: Option<Filter>,
}

impl SearchQuery {
    /// Creates a vector search request without metadata filters.
    pub fn new(embedding: Vec<f32>, top_k: usize) -> Self {
        Self {
            embedding,
            top_k,
            filter: None,
        }
    }

    /// Adds a metadata filter to the search request.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }
}

/// Single ranked search result.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub chunk_id: ChunkId,
    pub document_id: String,
    pub score: f32,
    pub trace: SearchTrace,
}

/// Debug data explaining why a chunk appeared in the result set.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchTrace {
    pub vector_score: f32,
    pub keyword_score: Option<f32>,
    pub filter_matched: bool,
}

/// BM25 keyword search request.
#[derive(Debug, Clone, PartialEq)]
pub struct KeywordQuery {
    pub text: String,
    pub top_k: usize,
    pub filter: Option<Filter>,
}

impl KeywordQuery {
    /// Creates a keyword search request without metadata filters.
    pub fn new(text: impl Into<String>, top_k: usize) -> Self {
        Self {
            text: text.into(),
            top_k,
            filter: None,
        }
    }

    /// Adds a metadata filter to the keyword search request.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }
}

/// Single ranked BM25 search result.
#[derive(Debug, Clone, PartialEq)]
pub struct KeywordHit {
    pub chunk_id: ChunkId,
    pub document_id: String,
    pub score: f32,
    pub matched_terms: Vec<String>,
}

/// Hybrid vector + BM25 search request.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridQuery {
    pub text: String,
    pub embedding: Vec<f32>,
    pub top_k: usize,
    pub vector_top_k: usize,
    pub keyword_top_k: usize,
    pub filter: Option<Filter>,
    pub fusion: HybridFusion,
}

impl HybridQuery {
    /// Creates a hybrid search request with stable V1 candidate defaults.
    pub fn new(text: impl Into<String>, embedding: Vec<f32>, top_k: usize) -> Self {
        Self {
            text: text.into(),
            embedding,
            top_k,
            vector_top_k: 50,
            keyword_top_k: 50,
            filter: None,
            fusion: HybridFusion::WeightedNormalizedScore {
                vector_weight: 0.6,
                keyword_weight: 0.4,
            },
        }
    }

    /// Adds a metadata filter to both vector and BM25 candidate generation.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Sets the number of vector and keyword candidates fused before final top-k.
    pub fn with_candidate_limits(mut self, vector_top_k: usize, keyword_top_k: usize) -> Self {
        self.vector_top_k = vector_top_k;
        self.keyword_top_k = keyword_top_k;
        self
    }

    /// Sets the RRF smoothing constant used by reciprocal rank fusion.
    pub fn with_rrf_k(mut self, rrf_k: f32) -> Self {
        self.fusion = HybridFusion::ReciprocalRank { rrf_k };
        self
    }

    /// Uses min-max normalized vector and BM25 scores with explicit weights.
    pub fn with_weighted_normalized_score(
        mut self,
        vector_weight: f32,
        keyword_weight: f32,
    ) -> Self {
        self.fusion = HybridFusion::WeightedNormalizedScore {
            vector_weight,
            keyword_weight,
        };
        self
    }
}

/// Hybrid rank fusion strategy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HybridFusion {
    ReciprocalRank {
        rrf_k: f32,
    },
    WeightedNormalizedScore {
        vector_weight: f32,
        keyword_weight: f32,
    },
}

/// Single ranked hybrid search result.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridHit {
    pub chunk_id: ChunkId,
    pub document_id: String,
    pub score: f32,
    pub vector_score: Option<f32>,
    pub keyword_score: Option<f32>,
    pub trace: HybridTrace,
}

/// Debug data explaining vector, keyword, and fusion contributions.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridTrace {
    pub vector_rank: Option<usize>,
    pub keyword_rank: Option<usize>,
    pub normalized_vector_score: Option<f32>,
    pub normalized_keyword_score: Option<f32>,
    pub matched_terms: Vec<String>,
    pub fusion: HybridFusionTrace,
    pub filter_matched: bool,
}

/// Fusion data copied into traces so callers can reproduce the score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HybridFusionTrace {
    ReciprocalRank {
        rrf_k: f32,
    },
    WeightedNormalizedScore {
        vector_weight: f32,
        keyword_weight: f32,
    },
}

impl From<HybridFusion> for HybridFusionTrace {
    fn from(fusion: HybridFusion) -> Self {
        match fusion {
            HybridFusion::ReciprocalRank { rrf_k } => Self::ReciprocalRank { rrf_k },
            HybridFusion::WeightedNormalizedScore {
                vector_weight,
                keyword_weight,
            } => Self::WeightedNormalizedScore {
                vector_weight,
                keyword_weight,
            },
        }
    }
}

/// Configuration for an exact vector index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexConfig {
    pub dimension: usize,
    pub metric: VectorMetric,
    pub vector_encoding: VectorEncoding,
}

impl IndexConfig {
    /// Creates an index configuration using `F32` vector storage.
    pub fn new(dimension: usize, metric: VectorMetric) -> Self {
        Self {
            dimension,
            metric,
            vector_encoding: VectorEncoding::F32,
        }
    }

    /// Sets the stored vector representation.
    pub fn with_vector_encoding(mut self, vector_encoding: VectorEncoding) -> Self {
        self.vector_encoding = vector_encoding;
        self
    }
}

/// Vector scoring mode used by exact search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorMetric {
    DotProduct,
    Cosine,
}

/// Stored vector representation used by an index.
///
/// Public callers can continue to provide `f32` embeddings while the index
/// chooses a storage/scoring representation. `BinaryQuantized` represents the
/// future 1-bit-per-dimension form, such as 768 bits for a 768-dimensional
/// embedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorEncoding {
    F32,
    F16,
    BF16,
    I8ScalarQuantized,
    BinaryQuantized,
}

impl VectorEncoding {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::F32 => "F32",
            Self::F16 => "F16",
            Self::BF16 => "BF16",
            Self::I8ScalarQuantized => "I8ScalarQuantized",
            Self::BinaryQuantized => "BinaryQuantized",
        }
    }
}

/// Approximate byte breakdown for the currently loaded index payload.
///
/// This intentionally reports payload bytes, not allocator or `BTreeMap`
/// overhead. It is useful for tracking the storage model while persistence is
/// being designed, and should be replaced or supplemented with real file sizes
/// once saved index files exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexSizeEstimate {
    pub vector_bytes: usize,
    pub chunk_record_bytes: usize,
    pub document_id_bytes: usize,
    pub text_bytes: usize,
    pub metadata_bytes: usize,
    pub tombstone_bytes: usize,
    pub version_bytes: usize,
    pub chunk_offset_bytes: usize,
    pub bm25_bytes: usize,
    pub metadata_filter_bytes: usize,
}

impl IndexSizeEstimate {
    pub fn chunk_bytes(&self) -> usize {
        self.chunk_record_bytes
            + self.document_id_bytes
            + self.text_bytes
            + self.metadata_bytes
            + self.tombstone_bytes
            + self.version_bytes
            + self.chunk_offset_bytes
    }

    pub fn auxiliary_bytes(&self) -> usize {
        self.chunk_bytes() + self.bm25_bytes + self.metadata_filter_bytes
    }

    pub fn total_bytes(&self) -> usize {
        self.vector_bytes + self.auxiliary_bytes()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexFileSizeReport {
    pub manifest_bytes: u64,
    pub vectors_bytes: u64,
    pub chunks_bytes: u64,
    pub bm25_bytes: u64,
    pub tombstones_bytes: u64,
}

impl IndexFileSizeReport {
    pub fn total_bytes(&self) -> u64 {
        self.manifest_bytes
            + self.vectors_bytes
            + self.chunks_bytes
            + self.bm25_bytes
            + self.tombstones_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexPersistenceOptions {
    pub include_bm25: bool,
}

impl IndexPersistenceOptions {
    pub fn hybrid() -> Self {
        Self { include_bm25: true }
    }

    pub fn vector_only() -> Self {
        Self {
            include_bm25: false,
        }
    }
}

impl Default for IndexPersistenceOptions {
    fn default() -> Self {
        Self::hybrid()
    }
}
