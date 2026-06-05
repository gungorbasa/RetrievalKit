use crate::filter::Filter;
use crate::metadata::Metadata;

pub type ChunkId = u64;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: String,
    pub text: String,
    pub metadata: Metadata,
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkInput {
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    pub embedding: Vec<f32>,
    pub top_k: usize,
    pub filter: Option<Filter>,
}

impl SearchQuery {
    pub fn new(embedding: Vec<f32>, top_k: usize) -> Self {
        Self {
            embedding,
            top_k,
            filter: None,
        }
    }

    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub chunk_id: ChunkId,
    pub document_id: String,
    pub score: f32,
    pub trace: SearchTrace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchTrace {
    pub vector_score: f32,
    pub keyword_score: Option<f32>,
    pub filter_matched: bool,
}

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
