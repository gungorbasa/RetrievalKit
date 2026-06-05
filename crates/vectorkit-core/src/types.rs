use crate::filter::Filter;
use crate::metadata::Metadata;

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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
