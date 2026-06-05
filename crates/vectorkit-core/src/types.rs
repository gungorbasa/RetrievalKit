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

/// Stored retrievable unit with an internal numeric ID.
///
/// Chunks are the search result unit. Callers should usually provide
/// `ChunkInput` values through `ExactVectorIndex::upsert_document` and let the
/// index assign `chunk_id` values.
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

/// Vector scoring mode used by exact search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorMetric {
    DotProduct,
    Cosine,
}

impl VectorMetric {
    pub(crate) fn score(self, query: &[f32], chunk: &[f32]) -> f32 {
        match self {
            Self::DotProduct => dot_product(query, chunk),
            Self::Cosine => cosine_similarity(query, chunk),
        }
    }
}

fn dot_product(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let dot = dot_product(left, right);
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();

    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }

    dot / (left_norm * right_norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_product_scores_vectors() {
        assert_eq!(
            VectorMetric::DotProduct.score(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]),
            32.0
        );
    }

    #[test]
    fn cosine_scores_vectors() {
        assert_eq!(VectorMetric::Cosine.score(&[1.0, 0.0], &[1.0, 0.0]), 1.0);
        assert_eq!(VectorMetric::Cosine.score(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }
}
