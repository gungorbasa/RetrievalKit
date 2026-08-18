// Encoded-vector byte payload helpers currently serve native persistence.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

#[cfg(target_arch = "wasm32")]
use half::{bf16, f16};
#[cfg(not(target_arch = "wasm32"))]
use simsimd::{bf16, f16, SpatialSimilarity};

use crate::error::{Result, RetrievalKitError};
use crate::types::{VectorEncoding, VectorMetric};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncodedVectorStore {
    F32(Vec<f32>),
    F16(Vec<f16>),
    BF16(Vec<bf16>),
    I8ScalarQuantized { values: Vec<i8>, scales: Vec<f32> },
}

impl EncodedVectorStore {
    pub fn new(encoding: VectorEncoding) -> Result<Self> {
        match encoding {
            VectorEncoding::F32 => Ok(Self::F32(Vec::new())),
            VectorEncoding::F16 => Ok(Self::F16(Vec::new())),
            VectorEncoding::BF16 => Ok(Self::BF16(Vec::new())),
            VectorEncoding::I8ScalarQuantized => Ok(Self::I8ScalarQuantized {
                values: Vec::new(),
                scales: Vec::new(),
            }),
        }
    }

    pub fn reserve_rows(&mut self, rows: usize, dimension: usize) {
        match self {
            Self::F32(vectors) => vectors.reserve(rows.saturating_mul(dimension)),
            Self::F16(vectors) => vectors.reserve(rows.saturating_mul(dimension)),
            Self::BF16(vectors) => vectors.reserve(rows.saturating_mul(dimension)),
            Self::I8ScalarQuantized { values, scales } => {
                values.reserve(rows.saturating_mul(dimension));
                scales.reserve(rows);
            }
        }
    }

    pub fn push(&mut self, embedding: &[f32]) {
        match self {
            Self::F32(vectors) => vectors.extend_from_slice(embedding),
            Self::F16(vectors) => {
                vectors.extend(embedding.iter().map(|&value| f16_from_f32(value)))
            }
            Self::BF16(vectors) => {
                vectors.extend(embedding.iter().map(|&value| bf16_from_f32(value)));
            }
            Self::I8ScalarQuantized { values, scales } => {
                let encoded = encode_i8_scalar_quantized(embedding);
                values.extend_from_slice(&encoded.values);
                scales.push(encoded.scale);
            }
        }
    }

    pub fn select_rows(&self, rows: &[usize], dimension: usize) -> Result<Self> {
        let mut selected = Self::new(match self {
            Self::F32(_) => VectorEncoding::F32,
            Self::F16(_) => VectorEncoding::F16,
            Self::BF16(_) => VectorEncoding::BF16,
            Self::I8ScalarQuantized { .. } => VectorEncoding::I8ScalarQuantized,
        })?;
        selected.reserve_rows(rows.len(), dimension);

        for &row in rows {
            let start = row
                .checked_mul(dimension)
                .ok_or_else(|| invalid_row(row, dimension))?;
            let end = start
                .checked_add(dimension)
                .ok_or_else(|| invalid_row(row, dimension))?;
            match (self, &mut selected) {
                (Self::F32(source), Self::F32(target)) => target.extend_from_slice(
                    source
                        .get(start..end)
                        .ok_or_else(|| invalid_row(row, dimension))?,
                ),
                (Self::F16(source), Self::F16(target)) => target.extend_from_slice(
                    source
                        .get(start..end)
                        .ok_or_else(|| invalid_row(row, dimension))?,
                ),
                (Self::BF16(source), Self::BF16(target)) => target.extend_from_slice(
                    source
                        .get(start..end)
                        .ok_or_else(|| invalid_row(row, dimension))?,
                ),
                (
                    Self::I8ScalarQuantized {
                        values: source_values,
                        scales: source_scales,
                    },
                    Self::I8ScalarQuantized {
                        values: target_values,
                        scales: target_scales,
                    },
                ) => {
                    target_values.extend_from_slice(
                        source_values
                            .get(start..end)
                            .ok_or_else(|| invalid_row(row, dimension))?,
                    );
                    target_scales.push(
                        *source_scales
                            .get(row)
                            .ok_or_else(|| invalid_row(row, dimension))?,
                    );
                }
                _ => unreachable!("selected store uses the source encoding"),
            }
        }

        Ok(selected)
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
            (
                Self::I8ScalarQuantized { values, scales },
                EncodedQuery::I8ScalarQuantized(query),
            ) => values
                .get(start..end)
                .zip(scales.get(row))
                .map(|(chunk, &chunk_scale)| {
                    score_i8_scalar_quantized(metric, query, chunk, chunk_scale)
                }),
            _ => None,
        }
    }

    pub(crate) fn i8_scalar_quantized_parts(&self) -> Option<(&[i8], &[f32])> {
        match self {
            Self::I8ScalarQuantized { values, scales } => Some((values, scales)),
            Self::F32(_) | Self::F16(_) | Self::BF16(_) => None,
        }
    }

    pub fn estimated_payload_bytes(&self) -> usize {
        match self {
            Self::F32(vectors) => vectors.len() * std::mem::size_of::<f32>(),
            Self::F16(vectors) => vectors.len() * std::mem::size_of::<f16>(),
            Self::BF16(vectors) => vectors.len() * std::mem::size_of::<bf16>(),
            Self::I8ScalarQuantized { values, scales } => {
                values.len() * std::mem::size_of::<i8>() + scales.len() * std::mem::size_of::<f32>()
            }
        }
    }

    pub fn to_payload_bytes(&self) -> Vec<u8> {
        match self {
            Self::F32(vectors) => vectors
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect(),
            Self::F16(vectors) => vectors
                .iter()
                .flat_map(|value| f16_bits(*value).to_le_bytes())
                .collect(),
            Self::BF16(vectors) => vectors
                .iter()
                .flat_map(|value| bf16_bits(*value).to_le_bytes())
                .collect(),
            Self::I8ScalarQuantized { values, scales } => {
                let mut bytes = Vec::with_capacity(
                    values.len() * std::mem::size_of::<i8>()
                        + scales.len() * std::mem::size_of::<f32>(),
                );
                bytes.extend(values.iter().map(|value| *value as u8));
                bytes.extend(scales.iter().flat_map(|value| value.to_le_bytes()));
                bytes
            }
        }
    }

    pub fn from_payload_bytes(
        encoding: VectorEncoding,
        vector_count: usize,
        dimension: usize,
        bytes: &[u8],
    ) -> Result<Self> {
        let value_count = vector_count.checked_mul(dimension).ok_or_else(|| {
            RetrievalKitError::InvalidFormat {
                message: "vector count and dimension overflow".to_owned(),
            }
        })?;

        match encoding {
            VectorEncoding::F32 => {
                expect_payload_len(bytes, value_count * std::mem::size_of::<f32>())?;
                Ok(Self::F32(
                    bytes
                        .chunks_exact(std::mem::size_of::<f32>())
                        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("chunk size")))
                        .collect(),
                ))
            }
            VectorEncoding::F16 => {
                expect_payload_len(bytes, value_count * std::mem::size_of::<f16>())?;
                Ok(Self::F16(
                    bytes
                        .chunks_exact(std::mem::size_of::<u16>())
                        .map(|chunk| {
                            f16_from_bits(u16::from_le_bytes(chunk.try_into().expect("chunk size")))
                        })
                        .collect(),
                ))
            }
            VectorEncoding::BF16 => {
                expect_payload_len(bytes, value_count * std::mem::size_of::<bf16>())?;
                Ok(Self::BF16(
                    bytes
                        .chunks_exact(std::mem::size_of::<u16>())
                        .map(|chunk| {
                            bf16_from_bits(u16::from_le_bytes(
                                chunk.try_into().expect("chunk size"),
                            ))
                        })
                        .collect(),
                ))
            }
            VectorEncoding::I8ScalarQuantized => {
                let values_len = value_count;
                let scales_len = vector_count * std::mem::size_of::<f32>();
                expect_payload_len(bytes, values_len + scales_len)?;
                let (values, scale_bytes) = bytes.split_at(values_len);
                Ok(Self::I8ScalarQuantized {
                    values: values.iter().map(|value| *value as i8).collect(),
                    scales: scale_bytes
                        .chunks_exact(std::mem::size_of::<f32>())
                        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("chunk size")))
                        .collect(),
                })
            }
        }
    }
}

fn invalid_row(row: usize, dimension: usize) -> RetrievalKitError {
    RetrievalKitError::InvalidFormat {
        message: format!(
            "vector row {row} is unavailable for compaction at dimension {dimension}; reload the index from its last saved snapshot before retrying compaction"
        ),
    }
}

fn expect_payload_len(bytes: &[u8], expected: usize) -> Result<()> {
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(RetrievalKitError::InvalidFormat {
            message: format!(
                "vector payload size mismatch: expected {expected} bytes, got {}",
                bytes.len()
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EncodedQuery {
    F32(Vec<f32>),
    F16(Vec<f16>),
    BF16(Vec<bf16>),
    I8ScalarQuantized(ScalarQuantizedVector),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScalarQuantizedVector {
    values: Vec<i8>,
    scale: f32,
}

impl EncodedQuery {
    pub(crate) fn i8_scalar_quantized_parts(&self) -> Option<(&[i8], f32)> {
        match self {
            Self::I8ScalarQuantized(query) => Some((&query.values, query.scale)),
            Self::F32(_) | Self::F16(_) | Self::BF16(_) => None,
        }
    }
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
        VectorEncoding::I8ScalarQuantized => Ok(EncodedQuery::I8ScalarQuantized(
            encode_i8_scalar_quantized(&embedding),
        )),
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

#[cfg(target_arch = "wasm32")]
fn score_f16(metric: VectorMetric, query: &[f16], chunk: &[f16]) -> f32 {
    let _ = metric;
    portable_dot_product_f16(query, chunk)
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
fn score_bf16(metric: VectorMetric, query: &[bf16], chunk: &[bf16]) -> f32 {
    let _ = metric;
    portable_dot_product_bf16(query, chunk)
}

#[cfg(not(target_arch = "wasm32"))]
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

fn score_i8_scalar_quantized(
    _metric: VectorMetric,
    query: &ScalarQuantizedVector,
    chunk: &[i8],
    chunk_scale: f32,
) -> f32 {
    simd_dot_product_i8(&query.values, chunk) * query.scale * chunk_scale
}

#[cfg(test)]
pub(crate) fn scalar_score(metric: VectorMetric, query: &[f32], chunk: &[f32]) -> f32 {
    match metric {
        VectorMetric::DotProduct => scalar_dot_product(query, chunk),
        VectorMetric::Cosine => scalar_cosine_similarity(query, chunk),
    }
}

#[cfg(target_arch = "wasm32")]
fn simd_dot_product(query: &[f32], chunk: &[f32]) -> f32 {
    scalar_dot_product(query, chunk)
}

#[cfg(not(target_arch = "wasm32"))]
fn simd_dot_product(query: &[f32], chunk: &[f32]) -> f32 {
    <f32 as SpatialSimilarity>::dot(query, chunk)
        .map(|distance| distance as f32)
        .filter(|score| score.is_finite())
        .unwrap_or_else(|| scalar_dot_product(query, chunk))
}

fn simd_dot_product_i8(query: &[i8], chunk: &[i8]) -> f32 {
    dot_product_i8(query, chunk)
}

#[doc(hidden)]
pub fn dot_product_i8(left: &[i8], right: &[i8]) -> f32 {
    if left.len() != right.len() {
        return scalar_dot_product_i8(left, right);
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("dotprod") {
            return unsafe { aarch64_dot_product_i8_neon(left, right) };
        }
    }

    #[cfg(all(target_arch = "wasm32", feature = "wasm-simd128"))]
    {
        // SAFETY: this code is emitted only in the separately built SIMD128
        // browser artifact. Its Worker loader validates SIMD128 support before
        // instantiating that artifact.
        unsafe { wasm_simd128_dot_product_i8(left, right) }
    }

    #[cfg(all(target_arch = "wasm32", not(feature = "wasm-simd128")))]
    {
        scalar_dot_product_i8(left, right)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        <i8 as SpatialSimilarity>::dot(left, right)
            .map(|distance| distance as f32)
            .filter(|score| score.is_finite())
            .unwrap_or_else(|| scalar_dot_product_i8(left, right))
    }
}

/// Exact signed-I8 dot product for the separately distributed SIMD128 WASM
/// artifact.
///
/// The main loop reads 16 valid I8 values per iteration, widens products to
/// I16, then pairwise-widens accumulation into I32 lanes. The V1 384d/768d
/// dimensions are far below I32 overflow. Any non-multiple-of-16 tail is
/// accumulated scalarly, so inferred dimensions such as 396 remain correct.
#[cfg(all(target_arch = "wasm32", feature = "wasm-simd128"))]
#[target_feature(enable = "simd128")]
unsafe fn wasm_simd128_dot_product_i8(left: &[i8], right: &[i8]) -> f32 {
    use core::arch::wasm32::{
        i16x8_extmul_high_i8x16, i16x8_extmul_low_i8x16, i32x4_add, i32x4_extadd_pairwise_i16x8,
        i32x4_extract_lane, i32x4_splat, v128, v128_load,
    };

    let vectorized_len = left.len() / 16 * 16;
    let mut sums = i32x4_splat(0);
    let mut offset = 0usize;
    while offset < vectorized_len {
        // SAFETY: `offset + 16 <= vectorized_len <= left.len()` and the same
        // length was established for `right` before this function was called.
        let left_values = unsafe { v128_load(left.as_ptr().add(offset).cast::<v128>()) };
        // SAFETY: see the left load above.
        let right_values = unsafe { v128_load(right.as_ptr().add(offset).cast::<v128>()) };
        let low_products = i16x8_extmul_low_i8x16(left_values, right_values);
        let high_products = i16x8_extmul_high_i8x16(left_values, right_values);
        sums = i32x4_add(sums, i32x4_extadd_pairwise_i16x8(low_products));
        sums = i32x4_add(sums, i32x4_extadd_pairwise_i16x8(high_products));
        offset += 16;
    }

    let vector_sum = i32x4_extract_lane::<0>(sums)
        + i32x4_extract_lane::<1>(sums)
        + i32x4_extract_lane::<2>(sums)
        + i32x4_extract_lane::<3>(sums);
    let tail_sum = left[vectorized_len..]
        .iter()
        .zip(&right[vectorized_len..])
        .map(|(left, right)| i32::from(*left) * i32::from(*right))
        .sum::<i32>();
    (vector_sum + tail_sum) as f32
}

#[cfg(target_arch = "aarch64")]
unsafe fn aarch64_dot_product_i8_neon(left: &[i8], right: &[i8]) -> f32 {
    retrievalkit_dot_i8_aarch64_dotprod(left.as_ptr(), right.as_ptr(), left.len()) as f32
}

#[cfg(target_arch = "aarch64")]
extern "C" {
    fn retrievalkit_dot_i8_aarch64_dotprod(left: *const i8, right: *const i8, length: usize)
        -> i32;
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
    embedding.iter().map(|&value| f16_from_f32(value)).collect()
}

fn encode_bf16(embedding: &[f32]) -> Vec<bf16> {
    embedding
        .iter()
        .map(|&value| bf16_from_f32(value))
        .collect()
}

#[cfg(any(test, target_arch = "wasm32"))]
fn portable_dot_product_f16(left: &[f16], right: &[f16]) -> f32 {
    let score = left
        .iter()
        .zip(right)
        .map(|(left, right)| left.to_f32() * right.to_f32())
        .sum::<f32>();
    if score.is_finite() {
        score
    } else {
        0.0
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
fn portable_dot_product_bf16(left: &[bf16], right: &[bf16]) -> f32 {
    let score = left
        .iter()
        .zip(right)
        .map(|(left, right)| left.to_f32() * right.to_f32())
        .sum::<f32>();
    if score.is_finite() {
        score
    } else {
        0.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn f16_bits(value: f16) -> u16 {
    value.0
}

#[cfg(target_arch = "wasm32")]
fn f16_bits(value: f16) -> u16 {
    value.to_bits()
}

#[cfg(not(target_arch = "wasm32"))]
fn f16_from_f32(value: f32) -> f16 {
    f16::from_f32(value)
}

#[cfg(target_arch = "wasm32")]
fn f16_from_f32(value: f32) -> f16 {
    f16::from_bits(portable_f32_to_f16_bits(value))
}

#[cfg(not(target_arch = "wasm32"))]
fn f16_from_bits(bits: u16) -> f16 {
    f16(bits)
}

#[cfg(target_arch = "wasm32")]
fn f16_from_bits(bits: u16) -> f16 {
    f16::from_bits(bits)
}

#[cfg(not(target_arch = "wasm32"))]
fn bf16_bits(value: bf16) -> u16 {
    value.0
}

#[cfg(target_arch = "wasm32")]
fn bf16_bits(value: bf16) -> u16 {
    value.to_bits()
}

#[cfg(not(target_arch = "wasm32"))]
fn bf16_from_f32(value: f32) -> bf16 {
    bf16::from_f32(value)
}

#[cfg(target_arch = "wasm32")]
fn bf16_from_f32(value: f32) -> bf16 {
    bf16::from_bits(portable_f32_to_bf16_bits(value))
}

#[cfg(not(target_arch = "wasm32"))]
fn bf16_from_bits(bits: u16) -> bf16 {
    bf16(bits)
}

#[cfg(target_arch = "wasm32")]
fn bf16_from_bits(bits: u16) -> bf16 {
    bf16::from_bits(bits)
}

#[cfg(any(test, target_arch = "wasm32"))]
fn portable_f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits().wrapping_add(0x0000_1000);
    let exponent = (bits & 0x7f80_0000) >> 23;
    let mantissa = bits & 0x007f_ffff;
    let mut result = (bits & 0x8000_0000) >> 16;
    if exponent > 112 {
        result |= ((exponent - 112) << 10 & 0x7c00) | (mantissa >> 13);
    }
    if exponent < 113 && exponent > 101 {
        result |= (((0x007f_f000 + mantissa) >> (125 - exponent)) + 1) >> 1;
    }
    if exponent > 143 {
        result |= 0x7fff;
    }
    result as u16
}

#[cfg(any(test, target_arch = "wasm32"))]
fn portable_f32_to_bf16_bits(value: f32) -> u16 {
    value.to_bits().wrapping_add(0x8000).wrapping_shr(16) as u16
}

fn encode_i8_scalar_quantized(embedding: &[f32]) -> ScalarQuantizedVector {
    let max_abs = embedding
        .iter()
        .map(|value| value.abs())
        .fold(0.0, f32::max);

    if max_abs == 0.0 {
        return ScalarQuantizedVector {
            values: vec![0; embedding.len()],
            scale: 0.0,
        };
    }

    let scale = max_abs / i8::MAX as f32;
    let inverse_scale = scale.recip();
    let values = embedding
        .iter()
        .map(|value| {
            (value * inverse_scale)
                .round()
                .clamp(i8::MIN as f32, i8::MAX as f32) as i8
        })
        .collect();

    ScalarQuantizedVector { values, scale }
}

fn scalar_dot_product(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn scalar_dot_product_i8(left: &[i8], right: &[i8]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| *left as f32 * *right as f32)
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
    fn wasm_half_encodings_match_native_simsimd_encodings() {
        let values = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.333_251_95,
            65_504.0,
            f32::MIN_POSITIVE,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        for value in values {
            assert_eq!(f16::from_f32(value).0, portable_f32_to_f16_bits(value));
            assert_eq!(bf16::from_f32(value).0, portable_f32_to_bf16_bits(value));
        }
    }

    #[test]
    fn wasm_portable_half_scores_match_native_simsimd_scores() {
        let left = [1.0, -2.25, 3.5, 0.125, 8.0];
        let right = [-4.0, 5.5, 0.75, 2.0, -1.0];
        let left_f16 = encode_f16(&left);
        let right_f16 = encode_f16(&right);
        let left_bf16 = encode_bf16(&left);
        let right_bf16 = encode_bf16(&right);

        assert_close(
            portable_dot_product_f16(&left_f16, &right_f16),
            <f16 as SpatialSimilarity>::dot(&left_f16, &right_f16).unwrap() as f32,
        );
        assert_close(
            portable_dot_product_bf16(&left_bf16, &right_bf16),
            <bf16 as SpatialSimilarity>::dot(&left_bf16, &right_bf16).unwrap() as f32,
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
    fn i8_scalar_quantized_vectors_score_with_rescaled_dot_product() {
        let mut vectors = EncodedVectorStore::new(VectorEncoding::I8ScalarQuantized).unwrap();
        vectors.push(&[1.0, 0.0]);
        let query = encode_query(VectorEncoding::I8ScalarQuantized, &[1.0, 0.0]).unwrap();

        assert_close(
            vectors
                .score_at(VectorMetric::Cosine, &query, 0, 2)
                .unwrap(),
            1.0,
        );
    }

    #[test]
    fn i8_scalar_quantized_zero_vectors_score_zero() {
        let mut vectors = EncodedVectorStore::new(VectorEncoding::I8ScalarQuantized).unwrap();
        vectors.push(&[0.0, 0.0]);
        let query = encode_query(VectorEncoding::I8ScalarQuantized, &[1.0, 0.0]).unwrap();

        assert_eq!(
            vectors
                .score_at(VectorMetric::Cosine, &query, 0, 2)
                .unwrap(),
            0.0
        );
    }

    #[test]
    fn i8_scalar_quantized_store_scores_rows_by_offset() {
        let mut vectors = EncodedVectorStore::new(VectorEncoding::I8ScalarQuantized).unwrap();
        vectors.push(&[1.0, 0.0]);
        vectors.push(&[0.0, 1.0]);
        let query = encode_query(VectorEncoding::I8ScalarQuantized, &[0.0, 1.0]).unwrap();

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

    #[test]
    fn i8_dot_product_matches_scalar_for_tail_lengths() {
        let left = [-4, -3, -2, -1, 0, 1, 2, 3, 4, 5, -6, 7, -8, 9, -10, 11, 12];
        let right = [12, -11, 10, -9, 8, -7, 6, -5, 4, -3, 2, -1, 0, 1, -2, 3, -4];

        assert_close(
            simd_dot_product_i8(&left, &right),
            scalar_dot_product_i8(&left, &right),
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
