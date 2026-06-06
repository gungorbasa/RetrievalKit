use std::error::Error;
use std::ffi::{CStr, CString};
use std::fmt::{Display, Formatter};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use simsimd::capabilities;
use vectorkit_core::{
    Chunk, ExactVectorIndex, Filter, IndexConfig, Metadata, MetadataValue, SearchHit, SearchQuery,
    VectorEncoding, VectorMetric,
};

const BENCH_FILTER_FIELD: &str = "__bench_filter_bucket";

/// Runs VectorKit's synthetic device benchmark and returns a heap-allocated
/// UTF-8 JSON string. Call `vectorkit_string_free` when the caller is done.
///
/// Passing a null pointer or an empty string uses the default config:
/// 24K chunks, dimensions 384 and 768, 200 queries, top_k 10, encodings
/// f32/f16/i8, unfiltered search, and filtered search with filter_every 10.
///
/// # Safety
///
/// `config_json`, when non-null, must point to a valid null-terminated UTF-8 C
/// string that remains alive for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn vectorkit_bench_synthetic_json(config_json: *const c_char) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let config = unsafe { read_config(config_json) }?;
        run_benchmark(config)
    }))
    .unwrap_or(Err(BenchError::Panic));

    json_to_c_string(&response_json(response))
}

/// Frees a string returned by `vectorkit_bench_synthetic_json`.
///
/// # Safety
///
/// `ptr` must be null or a pointer returned by `vectorkit_bench_synthetic_json`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn vectorkit_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        drop(CString::from_raw(ptr));
    }
}

unsafe fn read_config(config_json: *const c_char) -> Result<BenchmarkConfig, BenchError> {
    if config_json.is_null() {
        return Ok(BenchmarkConfig::default());
    }

    let raw = unsafe { CStr::from_ptr(config_json) }
        .to_str()
        .map_err(|_| BenchError::InvalidConfig("config_json must be valid UTF-8".to_owned()))?;

    if raw.trim().is_empty() {
        return Ok(BenchmarkConfig::default());
    }

    let config = serde_json::from_str(raw)?;
    validate_config(&config)?;
    Ok(config)
}

fn run_benchmark(config: BenchmarkConfig) -> Result<BenchmarkReport, BenchError> {
    validate_config(&config)?;
    let started_at = Instant::now();
    let mut runs = Vec::new();

    for &dimension in &config.dimensions {
        let f32_unfiltered =
            build_synthetic_index(&config, dimension, VectorEncoding::F32, None)?.0;
        let f32_filtered = match config.filter_every {
            Some(filter_every) if config.include_filtered => Some(build_synthetic_index(
                &config,
                dimension,
                VectorEncoding::F32,
                Some(filter_every),
            )?),
            Some(_) | None => None,
        };

        for &encoding in &config.encodings {
            if config.include_unfiltered {
                let run = benchmark_one(&config, dimension, encoding, None, Some(&f32_unfiltered))?;
                runs.push(run);
            }

            if config.include_filtered {
                if let Some((ground_truth, _)) = &f32_filtered {
                    let run = benchmark_one(
                        &config,
                        dimension,
                        encoding,
                        config.filter_every,
                        Some(ground_truth),
                    )?;
                    runs.push(run);
                }
            }
        }
    }

    Ok(BenchmarkReport {
        schema_version: 1,
        config,
        capabilities: RuntimeCapabilities::detect(),
        elapsed_ms: millis(started_at.elapsed()),
        runs,
    })
}

fn benchmark_one(
    config: &BenchmarkConfig,
    dimension: usize,
    encoding: VectorEncoding,
    filter_every: Option<usize>,
    ground_truth: Option<&ExactVectorIndex>,
) -> Result<BenchmarkRun, BenchError> {
    let (index, build_duration) = build_synthetic_index(config, dimension, encoding, filter_every)?;
    let index_size = index.size_estimate();
    let mut query_durations = Vec::with_capacity(config.queries);
    let mut recall_sum = 0.0;
    let mut total_hits = 0usize;
    let mut top_hit_checksum = 0u64;

    for query_id in 0..config.queries {
        let target_chunk = target_chunk_id(query_id, config.chunks);
        let query = generate_query_vector(dimension, config.seed, target_chunk as u64, query_id);
        let search_query =
            synthetic_search_query(config.top_k, query.clone(), filter_every, target_chunk);

        let start = Instant::now();
        let hits = index.search(&search_query)?;
        query_durations.push(start.elapsed());

        recall_sum += match ground_truth {
            Some(ground_truth) if encoding != VectorEncoding::F32 => {
                let ground_truth_query =
                    synthetic_search_query(config.top_k, query, filter_every, target_chunk);
                let ground_truth_hits = ground_truth.search(&ground_truth_query)?;
                recall_at_k(&hits, &ground_truth_hits)
            }
            Some(_) | None => 1.0,
        };

        total_hits += hits.len();
        if let Some(hit) = hits.first() {
            top_hit_checksum = top_hit_checksum.wrapping_add(hit.chunk_id);
        }
    }

    Ok(BenchmarkRun {
        chunks: config.chunks,
        dimension,
        top_k: config.top_k,
        encoding: encoding_name(encoding).to_owned(),
        metric: metric_name(config.metric).to_owned(),
        filter_every,
        vector_payload_bytes: index_size.vector_bytes,
        total_payload_bytes: index_size.total_bytes(),
        build_ms: millis(build_duration),
        min_ms: millis(*query_durations.iter().min().unwrap_or(&Duration::ZERO)),
        avg_ms: millis(average_duration(&query_durations)),
        p50_ms: millis(percentile(query_durations.clone(), 50)),
        p95_ms: millis(percentile(query_durations.clone(), 95)),
        max_ms: millis(*query_durations.iter().max().unwrap_or(&Duration::ZERO)),
        recall_at_k_vs_f32: recall_sum / config.queries as f64,
        total_hits,
        top_hit_checksum,
    })
}

fn build_synthetic_index(
    config: &BenchmarkConfig,
    dimension: usize,
    encoding: VectorEncoding,
    filter_every: Option<usize>,
) -> Result<(ExactVectorIndex, Duration), BenchError> {
    let index_config = IndexConfig::new(dimension, config.metric).with_vector_encoding(encoding);
    let mut index = ExactVectorIndex::try_with_config(index_config)?;

    let build_start = Instant::now();
    for chunk_id in 0..config.chunks {
        index.add_chunk(synthetic_chunk(
            dimension,
            config.seed,
            chunk_id,
            filter_every,
        ))?;
    }

    Ok((index, build_start.elapsed()))
}

fn synthetic_chunk(
    dimension: usize,
    seed: u64,
    chunk_id: usize,
    filter_every: Option<usize>,
) -> Chunk {
    let mut metadata = Metadata::new();
    if let Some(filter_every) = filter_every {
        metadata.insert(
            BENCH_FILTER_FIELD.to_owned(),
            MetadataValue::Integer((chunk_id % filter_every) as i64),
        );
    }

    Chunk {
        chunk_id: chunk_id as u64,
        document_id: format!("synthetic-doc-{chunk_id}"),
        text: format!("synthetic chunk {chunk_id} topic {}", chunk_id % 17),
        embedding: generate_normalized_vector(dimension, seed, chunk_id as u64),
        metadata,
        deleted: false,
        version: 1,
    }
}

fn synthetic_search_query(
    top_k: usize,
    embedding: Vec<f32>,
    filter_every: Option<usize>,
    target_chunk: usize,
) -> SearchQuery {
    let query = SearchQuery::new(embedding, top_k);

    match filter_every {
        Some(filter_every) => query.with_filter(Filter::Equals {
            field: BENCH_FILTER_FIELD.to_owned(),
            value: MetadataValue::Integer((target_chunk % filter_every) as i64),
        }),
        None => query,
    }
}

fn recall_at_k(hits: &[SearchHit], ground_truth_hits: &[SearchHit]) -> f64 {
    if ground_truth_hits.is_empty() {
        return 1.0;
    }

    let matching_hits = hits
        .iter()
        .filter(|hit| {
            ground_truth_hits
                .iter()
                .any(|ground_truth_hit| ground_truth_hit.chunk_id == hit.chunk_id)
        })
        .count();

    matching_hits as f64 / ground_truth_hits.len() as f64
}

fn generate_query_vector(
    dimension: usize,
    seed: u64,
    target_chunk: u64,
    query_id: usize,
) -> Vec<f32> {
    let mut vector = generate_normalized_vector(dimension, seed, target_chunk);
    let mut noise = DeterministicRng::new(seed ^ 0x9e37_79b9_7f4a_7c15 ^ query_id as u64);

    for value in &mut vector {
        *value += noise.next_f32_signed() * 0.01;
    }

    normalize(&mut vector);
    vector
}

fn generate_normalized_vector(dimension: usize, seed: u64, vector_id: u64) -> Vec<f32> {
    let mut rng = DeterministicRng::new(seed ^ vector_id.wrapping_mul(0xbf58_476d_1ce4_e5b9));
    let mut vector = Vec::with_capacity(dimension);
    for _ in 0..dimension {
        vector.push(rng.next_f32_signed());
    }
    normalize(&mut vector);
    vector
}

fn normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();

    if norm == 0.0 {
        return;
    }

    for value in vector {
        *value /= norm;
    }
}

fn target_chunk_id(query_id: usize, chunks: usize) -> usize {
    query_id.wrapping_mul(9_973) % chunks
}

fn percentile(mut durations: Vec<Duration>, percentile: usize) -> Duration {
    durations.sort_unstable();
    let index = ((durations.len() - 1) * percentile) / 100;
    durations[index]
}

fn average_duration(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }

    let total_nanos = durations.iter().map(Duration::as_nanos).sum::<u128>();
    Duration::from_nanos((total_nanos / durations.len() as u128) as u64)
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn validate_config(config: &BenchmarkConfig) -> Result<(), BenchError> {
    if config.chunks == 0 {
        return Err(BenchError::InvalidConfig(
            "chunks must be greater than zero".to_owned(),
        ));
    }
    if config.dimensions.is_empty() || config.dimensions.contains(&0) {
        return Err(BenchError::InvalidConfig(
            "dimensions must contain positive values".to_owned(),
        ));
    }
    if config.queries == 0 {
        return Err(BenchError::InvalidConfig(
            "queries must be greater than zero".to_owned(),
        ));
    }
    if config.top_k == 0 {
        return Err(BenchError::InvalidConfig(
            "top_k must be greater than zero".to_owned(),
        ));
    }
    if config.encodings.is_empty() {
        return Err(BenchError::InvalidConfig(
            "encodings must contain at least one encoding".to_owned(),
        ));
    }
    if config.include_filtered && config.filter_every.is_none() {
        return Err(BenchError::InvalidConfig(
            "filter_every is required when include_filtered is true".to_owned(),
        ));
    }
    if matches!(config.filter_every, Some(0)) {
        return Err(BenchError::InvalidConfig(
            "filter_every must be greater than zero".to_owned(),
        ));
    }
    Ok(())
}

fn response_json(response: Result<BenchmarkReport, BenchError>) -> String {
    let value = match response {
        Ok(report) => FfiResponse {
            ok: true,
            report: Some(report),
            error: None,
        },
        Err(error) => FfiResponse {
            ok: false,
            report: None,
            error: Some(error.to_string()),
        },
    };

    serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error\":\"failed to serialize benchmark response\"}".to_owned()
    })
}

fn json_to_c_string(json: &str) -> *mut c_char {
    let sanitized = json.replace('\0', "\\u0000");
    match CString::new(sanitized) {
        Ok(value) => value.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

fn encoding_name(encoding: VectorEncoding) -> &'static str {
    match encoding {
        VectorEncoding::F32 => "f32",
        VectorEncoding::F16 => "f16",
        VectorEncoding::BF16 => "bf16",
        VectorEncoding::I8ScalarQuantized => "i8-scalar-quantized",
        VectorEncoding::BinaryQuantized => "binary-quantized",
    }
}

fn metric_name(metric: VectorMetric) -> &'static str {
    match metric {
        VectorMetric::DotProduct => "dot",
        VectorMetric::Cosine => "cosine",
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
struct BenchmarkConfig {
    chunks: usize,
    dimensions: Vec<usize>,
    queries: usize,
    top_k: usize,
    #[serde(with = "encoding_list_json")]
    encodings: Vec<VectorEncoding>,
    #[serde(with = "metric_json")]
    metric: VectorMetric,
    seed: u64,
    include_unfiltered: bool,
    include_filtered: bool,
    filter_every: Option<usize>,
}

mod encoding_list_json {
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};
    use vectorkit_core::VectorEncoding;

    use super::encoding_name;

    pub fn serialize<S>(encodings: &[VectorEncoding], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(encodings.iter().map(|&encoding| encoding_name(encoding)))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<VectorEncoding>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EncodingListVisitor;

        impl<'de> Visitor<'de> for EncodingListVisitor {
            type Value = Vec<VectorEncoding>;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a list of f32, f16, bf16, or i8")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut encodings = Vec::new();
                while let Some(value) = seq.next_element::<String>()? {
                    encodings.push(parse_encoding(&value).map_err(A::Error::custom)?);
                }
                Ok(encodings)
            }
        }

        deserializer.deserialize_seq(EncodingListVisitor)
    }

    fn parse_encoding(value: &str) -> Result<VectorEncoding, String> {
        match value.to_ascii_lowercase().as_str() {
            "f32" => Ok(VectorEncoding::F32),
            "f16" => Ok(VectorEncoding::F16),
            "bf16" => Ok(VectorEncoding::BF16),
            "i8" | "i8-scalar" | "i8-scalar-quantized" => Ok(VectorEncoding::I8ScalarQuantized),
            _ => Err(format!(
                "unsupported encoding '{value}', expected f32, f16, bf16, or i8"
            )),
        }
    }

    use std::fmt::Formatter;
}

mod metric_json {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};
    use vectorkit_core::VectorMetric;

    use super::metric_name;

    pub fn serialize<S>(metric: &VectorMetric, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(metric_name(*metric))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VectorMetric, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.to_ascii_lowercase().as_str() {
            "cosine" => Ok(VectorMetric::Cosine),
            "dot" | "dot-product" => Ok(VectorMetric::DotProduct),
            _ => Err(D::Error::custom(format!(
                "unsupported metric '{value}', expected cosine or dot"
            ))),
        }
    }
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            chunks: 24_000,
            dimensions: vec![384, 768],
            queries: 200,
            top_k: 10,
            encodings: vec![
                VectorEncoding::F32,
                VectorEncoding::F16,
                VectorEncoding::I8ScalarQuantized,
            ],
            metric: VectorMetric::Cosine,
            seed: 42,
            include_unfiltered: true,
            include_filtered: true,
            filter_every: Some(10),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    config: BenchmarkConfig,
    capabilities: RuntimeCapabilities,
    elapsed_ms: f64,
    runs: Vec<BenchmarkRun>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeCapabilities {
    simsimd: String,
    aarch64_dotprod: bool,
}

impl RuntimeCapabilities {
    fn detect() -> Self {
        Self {
            simsimd: simsimd_capability_summary(),
            aarch64_dotprod: aarch64_dotprod_detected(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkRun {
    chunks: usize,
    dimension: usize,
    top_k: usize,
    encoding: String,
    metric: String,
    filter_every: Option<usize>,
    vector_payload_bytes: usize,
    total_payload_bytes: usize,
    build_ms: f64,
    min_ms: f64,
    avg_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
    recall_at_k_vs_f32: f64,
    total_hits: usize,
    top_hit_checksum: u64,
}

#[derive(Debug, Clone, Serialize)]
struct FfiResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<BenchmarkReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug)]
enum BenchError {
    InvalidConfig(String),
    Json(serde_json::Error),
    Core(vectorkit_core::VectorKitError),
    Panic,
}

impl Display for BenchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "{message}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Core(error) => write!(f, "{error}"),
            Self::Panic => write!(f, "benchmark panicked"),
        }
    }
}

impl Error for BenchError {}

impl From<serde_json::Error> for BenchError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<vectorkit_core::VectorKitError> for BenchError {
    fn from(value: vectorkit_core::VectorKitError) -> Self {
        Self::Core(value)
    }
}

#[derive(Debug, Clone)]
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9e37_79b9_7f4a_7c15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn next_f32_signed(&mut self) -> f32 {
        let unit = (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32;
        unit.mul_add(2.0, -1.0)
    }
}

fn simsimd_capability_summary() -> String {
    [
        ("neon", capabilities::uses_neon()),
        ("neon_f16", capabilities::uses_neon_f16()),
        ("neon_i8", capabilities::uses_neon_i8()),
        ("sve", capabilities::uses_sve()),
        ("sve_f16", capabilities::uses_sve_f16()),
        ("sve_i8", capabilities::uses_sve_i8()),
        ("haswell", capabilities::uses_haswell()),
        ("skylake", capabilities::uses_skylake()),
        ("ice", capabilities::uses_ice()),
        ("sierra", capabilities::uses_sierra()),
        ("dynamic", capabilities::uses_dynamic_dispatch()),
    ]
    .into_iter()
    .filter_map(|(name, active)| active.then_some(name))
    .collect::<Vec<_>>()
    .join(",")
}

#[cfg(target_arch = "aarch64")]
fn aarch64_dotprod_detected() -> bool {
    std::arch::is_aarch64_feature_detected!("dotprod")
}

#[cfg(not(target_arch = "aarch64"))]
fn aarch64_dotprod_detected() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    #[test]
    fn default_config_includes_f32_f16_and_i8() {
        let config = BenchmarkConfig::default();

        assert_eq!(
            config.encodings,
            vec![
                VectorEncoding::F32,
                VectorEncoding::F16,
                VectorEncoding::I8ScalarQuantized,
            ]
        );
        assert_eq!(config.dimensions, vec![384, 768]);
        assert!(config.include_unfiltered);
        assert!(config.include_filtered);
    }

    #[test]
    fn benchmark_json_reports_all_requested_encoding_filter_pairs() {
        let config = BenchmarkConfig {
            chunks: 32,
            dimensions: vec![8],
            queries: 4,
            top_k: 3,
            filter_every: Some(4),
            ..BenchmarkConfig::default()
        };

        let report = run_benchmark(config).unwrap();

        assert_eq!(report.runs.len(), 6);
        assert!(report
            .runs
            .iter()
            .any(|run| run.encoding == "f32" && run.filter_every.is_none()));
        assert!(report
            .runs
            .iter()
            .any(|run| run.encoding == "f16" && run.filter_every == Some(4)));
        assert!(report
            .runs
            .iter()
            .any(|run| run.encoding == "i8-scalar-quantized" && run.filter_every == Some(4)));
    }

    #[test]
    fn invalid_config_returns_error_json() {
        let response = response_json(run_benchmark(BenchmarkConfig {
            chunks: 0,
            ..BenchmarkConfig::default()
        }));

        assert!(response.contains("\"ok\":false"));
        assert!(response.contains("chunks must be greater than zero"));
    }

    #[test]
    fn ffi_function_returns_json_and_owned_string_can_be_freed() {
        let config = CString::new(
            r#"{"chunks":16,"dimensions":[8],"queries":2,"top_k":2,"encodings":["f32","f16","i8"],"include_filtered":false}"#,
        )
        .unwrap();

        let ptr = unsafe { vectorkit_bench_synthetic_json(config.as_ptr()) };

        assert!(!ptr.is_null());
        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
        unsafe { vectorkit_string_free(ptr) };

        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"encoding\":\"f32\""));
        assert!(json.contains("\"encoding\":\"f16\""));
        assert!(json.contains("\"encoding\":\"i8-scalar-quantized\""));
    }
}
