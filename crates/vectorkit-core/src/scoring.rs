use simsimd::{bf16, f16, SpatialSimilarity};

use crate::error::{Result, VectorKitError};
use crate::types::{VectorEncoding, VectorMetric};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncodedVectorStore {
    F32(Vec<f32>),
    F16(Vec<f16>),
    BF16(Vec<bf16>),
}

impl EncodedVectorStore {
    pub fn new(encoding: VectorEncoding) -> Result<Self> {
        match encoding {
            VectorEncoding::F32 => Ok(Self::F32(Vec::new())),
            VectorEncoding::F16 => Ok(Self::F16(Vec::new())),
            VectorEncoding::BF16 => Ok(Self::BF16(Vec::new())),
            VectorEncoding::I8ScalarQuantized | VectorEncoding::BinaryQuantized => {
                Err(VectorKitError::UnsupportedVectorEncoding {
                    encoding: encoding.as_str().to_owned(),
                })
            }
        }
    }

    pub fn push(&mut self, embedding: &[f32]) {
        match self {
            Self::F32(vectors) => vectors.extend_from_slice(embedding),
            Self::F16(vectors) => {
                vectors.extend(embedding.iter().map(|&value| f16::from_f32(value)))
            }
            Self::BF16(vectors) => {
                vectors.extend(embedding.iter().map(|&value| bf16::from_f32(value)));
            }
        }
    }

    pub fn score_at(
        &self,
        metric: VectorMetric,
        query: &EncodedQuery,
        row: usize,
        dimension: usize,
    ) -> Option<f32> {
        let start = row.checked_mul(dimension)?;
        let end = start.checked_add(dimension)?;

        match (self, query) {
            (Self::F32(vectors), EncodedQuery::F32(query)) => vectors
                .get(start..end)
                .map(|chunk| score_f32(metric, query, chunk)),
            (Self::F16(vectors), EncodedQuery::F16(query)) => vectors
                .get(start..end)
                .map(|chunk| score_f16(metric, query, chunk)),
            (Self::BF16(vectors), EncodedQuery::BF16(query)) => vectors
                .get(start..end)
                .map(|chunk| score_bf16(metric, query, chunk)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncodedQuery {
    F32(Vec<f32>),
    F16(Vec<f16>),
    BF16(Vec<bf16>),
}

pub(crate) fn encode_query(encoding: VectorEncoding, embedding: &[f32]) -> Result<EncodedQuery> {
    encode_query_owned(encoding, embedding.to_vec())
}

pub(crate) fn encode_query_owned(
    encoding: VectorEncoding,
    embedding: Vec<f32>,
) -> Result<EncodedQuery> {
    match encoding {
        VectorEncoding::F32 => Ok(EncodedQuery::F32(embedding)),
        VectorEncoding::F16 => Ok(EncodedQuery::F16(encode_f16(&embedding))),
        VectorEncoding::BF16 => Ok(EncodedQuery::BF16(encode_bf16(&embedding))),
        VectorEncoding::I8ScalarQuantized | VectorEncoding::BinaryQuantized => {
            Err(VectorKitError::UnsupportedVectorEncoding {
                encoding: encoding.as_str().to_owned(),
            })
        }
    }
}

#[cfg(test)]
pub(crate) fn score(metric: VectorMetric, query: &[f32], chunk: &[f32]) -> f32 {
    score_f32(metric, query, chunk)
}

fn score_f32(metric: VectorMetric, query: &[f32], chunk: &[f32]) -> f32 {
    match metric {
        VectorMetric::DotProduct => simd_dot_product(query, chunk),
        VectorMetric::Cosine => simd_dot_product(query, chunk),
    }
}

fn score_f16(metric: VectorMetric, query: &[f16], chunk: &[f16]) -> f32 {
    match metric {
        VectorMetric::DotProduct => <f16 as SpatialSimilarity>::dot(query, chunk)
            .map(|distance| distance as f32)
            .filter(|score| score.is_finite())
            .unwrap_or(0.0),
        VectorMetric::Cosine => <f16 as SpatialSimilarity>::dot(query, chunk)
            .map(|distance| distance as f32)
            .filter(|score| score.is_finite())
            .unwrap_or(0.0),
    }
}

fn score_bf16(metric: VectorMetric, query: &[bf16], chunk: &[bf16]) -> f32 {
    match metric {
        VectorMetric::DotProduct => <bf16 as SpatialSimilarity>::dot(query, chunk)
            .map(|distance| distance as f32)
            .filter(|score| score.is_finite())
            .unwrap_or(0.0),
        VectorMetric::Cosine => <bf16 as SpatialSimilarity>::dot(query, chunk)
            .map(|distance| distance as f32)
            .filter(|score| score.is_finite())
            .unwrap_or(0.0),
    }
}

#[cfg(test)]
pub(crate) fn scalar_score(metric: VectorMetric, query: &[f32], chunk: &[f32]) -> f32 {
    match metric {
        VectorMetric::DotProduct => scalar_dot_product(query, chunk),
        VectorMetric::Cosine => scalar_cosine_similarity(query, chunk),
    }
}

fn simd_dot_product(query: &[f32], chunk: &[f32]) -> f32 {
    <f32 as SpatialSimilarity>::dot(query, chunk)
        .map(|distance| distance as f32)
        .filter(|score| score.is_finite())
        .unwrap_or_else(|| scalar_dot_product(query, chunk))
}

pub(crate) fn normalize(vector: &mut [f32]) {
    let squared_norm = scalar_dot_product(vector, vector);
    if squared_norm == 0.0 {
        return;
    }

    let inverse_norm = squared_norm.sqrt().recip();
    for value in vector {
        *value *= inverse_norm;
    }
}

fn encode_f16(embedding: &[f32]) -> Vec<f16> {
    embedding
        .iter()
        .map(|&value| f16::from_f32(value))
        .collect()
}

fn encode_bf16(embedding: &[f32]) -> Vec<bf16> {
    embedding
        .iter()
        .map(|&value| bf16::from_f32(value))
        .collect()
}

fn scalar_dot_product(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

#[cfg(test)]
fn scalar_cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let dot = scalar_dot_product(left, right);
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

    fn assert_close(left: f32, right: f32) {
        assert!(
            (left - right).abs() <= 1e-5,
            "expected {left} to be close to {right}"
        );
    }

    #[test]
    fn simsimd_dot_product_matches_scalar_score() {
        let left = [1.0, 2.0, 3.0, 4.0];
        let right = [5.0, 6.0, 7.0, 8.0];

        assert_close(
            score(VectorMetric::DotProduct, &left, &right),
            scalar_score(VectorMetric::DotProduct, &left, &right),
        );
    }

    #[test]
    fn normalized_cosine_uses_dot_product_score() {
        let left = [1.0, 2.0, 3.0, 4.0];
        let right = [4.0, 3.0, 2.0, 1.0];
        let mut normalized_left = left;
        let mut normalized_right = right;
        normalize(&mut normalized_left);
        normalize(&mut normalized_right);

        assert_close(
            score(VectorMetric::Cosine, &normalized_left, &normalized_right),
            scalar_score(VectorMetric::Cosine, &left, &right),
        );
    }

    #[test]
    fn scalar_cosine_returns_zero_for_zero_vector() {
        assert_eq!(
            scalar_score(VectorMetric::Cosine, &[0.0, 0.0], &[1.0, 0.0]),
            0.0
        );
    }

    #[test]
    fn normalized_cosine_dot_score_returns_zero_for_zero_vector() {
        assert_eq!(score(VectorMetric::Cosine, &[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    fn normalize_leaves_zero_vectors_unchanged() {
        let mut vector = [0.0, 0.0];

        normalize(&mut vector);

        assert_eq!(vector, [0.0, 0.0]);
    }

    #[test]
    fn normalize_scales_vectors_to_unit_length() {
        let mut vector = [3.0, 4.0];

        normalize(&mut vector);

        assert_close(scalar_dot_product(&vector, &vector), 1.0);
        assert_close(vector[0], 0.6);
        assert_close(vector[1], 0.8);
    }

    #[test]
    fn f16_encoded_vectors_score_without_decoding_index_to_f32() {
        let mut vectors = EncodedVectorStore::new(VectorEncoding::F16).unwrap();
        vectors.push(&[1.0, 0.0]);
        let query = encode_query(VectorEncoding::F16, &[1.0, 0.0]).unwrap();

        assert_close(
            vectors
                .score_at(VectorMetric::Cosine, &query, 0, 2)
                .unwrap(),
            1.0,
        );
    }

    #[test]
    fn bf16_encoded_vectors_score_without_decoding_index_to_f32() {
        let mut vectors = EncodedVectorStore::new(VectorEncoding::BF16).unwrap();
        vectors.push(&[1.0, 0.0]);
        let query = encode_query(VectorEncoding::BF16, &[1.0, 0.0]).unwrap();

        assert_close(
            vectors
                .score_at(VectorMetric::Cosine, &query, 0, 2)
                .unwrap(),
            1.0,
        );
    }

    #[test]
    fn unsupported_vector_encodings_return_errors() {
        assert_eq!(
            EncodedVectorStore::new(VectorEncoding::BinaryQuantized).unwrap_err(),
            VectorKitError::UnsupportedVectorEncoding {
                encoding: "BinaryQuantized".to_owned()
            }
        );
    }

    #[test]
    fn contiguous_store_scores_rows_by_offset() {
        let mut vectors = EncodedVectorStore::new(VectorEncoding::F32).unwrap();
        vectors.push(&[1.0, 0.0]);
        vectors.push(&[0.0, 1.0]);
        let query = encode_query(VectorEncoding::F32, &[0.0, 1.0]).unwrap();

        assert_close(
            vectors
                .score_at(VectorMetric::Cosine, &query, 0, 2)
                .unwrap(),
            0.0,
        );
        assert_close(
            vectors
                .score_at(VectorMetric::Cosine, &query, 1, 2)
                .unwrap(),
            1.0,
        );
    }
}
