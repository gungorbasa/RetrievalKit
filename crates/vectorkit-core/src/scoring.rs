use simsimd::{bf16, f16, SpatialSimilarity};

use crate::error::{Result, VectorKitError};
use crate::types::{VectorEncoding, VectorMetric};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncodedVector {
    F32(Vec<f32>),
    F16(Vec<f16>),
    BF16(Vec<bf16>),
}

impl EncodedVector {
    pub fn score(&self, metric: VectorMetric, query: &EncodedQuery) -> f32 {
        match (self, query) {
            (Self::F32(chunk), EncodedQuery::F32(query)) => score_f32(metric, query, chunk),
            (Self::F16(chunk), EncodedQuery::F16(query)) => score_f16(metric, query, chunk),
            (Self::BF16(chunk), EncodedQuery::BF16(query)) => score_bf16(metric, query, chunk),
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncodedQuery {
    F32(Vec<f32>),
    F16(Vec<f16>),
    BF16(Vec<bf16>),
}

pub(crate) fn encode_vector(encoding: VectorEncoding, embedding: &[f32]) -> Result<EncodedVector> {
    match encoding {
        VectorEncoding::F32 => Ok(EncodedVector::F32(embedding.to_vec())),
        VectorEncoding::F16 => Ok(EncodedVector::F16(encode_f16(embedding))),
        VectorEncoding::BF16 => Ok(EncodedVector::BF16(encode_bf16(embedding))),
        VectorEncoding::I8ScalarQuantized | VectorEncoding::BinaryQuantized => {
            Err(VectorKitError::UnsupportedVectorEncoding {
                encoding: encoding.as_str().to_owned(),
            })
        }
    }
}

pub(crate) fn encode_query(encoding: VectorEncoding, embedding: &[f32]) -> Result<EncodedQuery> {
    match encoding {
        VectorEncoding::F32 => Ok(EncodedQuery::F32(embedding.to_vec())),
        VectorEncoding::F16 => Ok(EncodedQuery::F16(encode_f16(embedding))),
        VectorEncoding::BF16 => Ok(EncodedQuery::BF16(encode_bf16(embedding))),
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
        VectorMetric::Cosine => simd_cosine_similarity(query, chunk),
    }
}

fn score_f16(metric: VectorMetric, query: &[f16], chunk: &[f16]) -> f32 {
    match metric {
        VectorMetric::DotProduct => <f16 as SpatialSimilarity>::dot(query, chunk)
            .map(|distance| distance as f32)
            .filter(|score| score.is_finite())
            .unwrap_or(0.0),
        VectorMetric::Cosine => <f16 as SpatialSimilarity>::cos(query, chunk)
            .map(|distance| 1.0 - distance as f32)
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
        VectorMetric::Cosine => <bf16 as SpatialSimilarity>::cos(query, chunk)
            .map(|distance| 1.0 - distance as f32)
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

fn simd_cosine_similarity(query: &[f32], chunk: &[f32]) -> f32 {
    <f32 as SpatialSimilarity>::cos(query, chunk)
        .map(|distance| 1.0 - distance as f32)
        .filter(|score| score.is_finite())
        .unwrap_or_else(|| scalar_cosine_similarity(query, chunk))
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
    fn simsimd_cosine_matches_scalar_score() {
        let left = [1.0, 2.0, 3.0, 4.0];
        let right = [4.0, 3.0, 2.0, 1.0];

        assert_close(
            score(VectorMetric::Cosine, &left, &right),
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
    fn simsimd_cosine_falls_back_for_zero_vector() {
        assert_eq!(score(VectorMetric::Cosine, &[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    fn f16_encoded_vectors_score_without_decoding_index_to_f32() {
        let chunk = encode_vector(VectorEncoding::F16, &[1.0, 0.0]).unwrap();
        let query = encode_query(VectorEncoding::F16, &[1.0, 0.0]).unwrap();

        assert_close(chunk.score(VectorMetric::Cosine, &query), 1.0);
    }

    #[test]
    fn bf16_encoded_vectors_score_without_decoding_index_to_f32() {
        let chunk = encode_vector(VectorEncoding::BF16, &[1.0, 0.0]).unwrap();
        let query = encode_query(VectorEncoding::BF16, &[1.0, 0.0]).unwrap();

        assert_close(chunk.score(VectorMetric::Cosine, &query), 1.0);
    }

    #[test]
    fn unsupported_vector_encodings_return_errors() {
        assert_eq!(
            encode_vector(VectorEncoding::BinaryQuantized, &[1.0]).unwrap_err(),
            VectorKitError::UnsupportedVectorEncoding {
                encoding: "BinaryQuantized".to_owned()
            }
        );
    }
}
