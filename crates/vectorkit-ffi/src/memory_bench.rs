use std::error::Error;
use std::ffi::CStr;
use std::fmt::{Display, Formatter};
use std::fs;
use std::hint::black_box;
use std::io;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vectorkit_core::{
    Chunk, CompactionReport, ExactVectorIndex, HybridQuery, IndexConfig, IndexFileSizeReport,
    IndexPersistenceOptions, Metadata, SearchQuery, VectorEncoding, VectorMetric,
};

use crate::bench::ProcessMemorySnapshot;
use crate::json_to_c_string;

/// Runs one isolated memory scenario and returns a heap-allocated UTF-8 JSON
/// string. The caller must release it with `vectorkit_string_free`.
///
/// Each invocation accepts exactly one chunk-count/dimension/encoding/workload
/// combination. Launch a fresh process for each scenario so the process high
/// water mark and allocator state are not inherited from an earlier run.
///
/// # Safety
///
/// `config_json`, when non-null, must point to a valid null-terminated UTF-8 C
/// string that remains alive for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn vectorkit_bench_memory_json(config_json: *const c_char) -> *mut c_char {
    let response = std::panic::catch_unwind(|| {
        let raw = if config_json.is_null() {
            ""
        } else {
            unsafe { CStr::from_ptr(config_json) }
                .to_str()
                .map_err(|_| {
                    MemoryBenchError::InvalidConfig("config_json must be valid UTF-8".to_owned())
                })?
        };
        Ok::<_, MemoryBenchError>(memory_benchmark_json(raw).0)
    })
    .unwrap_or_else(|_| Ok(error_response("memory benchmark panicked")));

    json_to_c_string(&response.unwrap_or_else(|error| error_response(&error.to_string())))
}

/// Safe entry point used by the benchmark CLI. The boolean is false when the
/// scenario ran successfully but exceeded one or more configured budgets.
pub fn memory_benchmark_json(config_json: &str) -> (String, bool) {
    let result = parse_config(config_json).and_then(run_memory_benchmark);
    match result {
        Ok(report) => {
            let passed = report.budgets.passed;
            (
                serde_json::to_string_pretty(&MemoryBenchmarkResponse {
                    ok: true,
                    report: Some(report),
                    error: None,
                })
                .unwrap_or_else(|_| {
                    error_response("failed to serialize memory benchmark response")
                }),
                passed,
            )
        }
        Err(error) => (error_response(&error.to_string()), false),
    }
}

fn parse_config(raw: &str) -> Result<MemoryBenchmarkConfig, MemoryBenchError> {
    let config = if raw.trim().is_empty() {
        MemoryBenchmarkConfig::default()
    } else {
        serde_json::from_str(raw)?
    };
    config.validate()?;
    Ok(config)
}

fn run_memory_benchmark(
    config: MemoryBenchmarkConfig,
) -> Result<MemoryBenchmarkReport, MemoryBenchError> {
    let baseline = ProcessMemorySnapshot::current();
    let directory = TemporaryDirectory::new(&config.scenario_id)?;
    let mut phases = Vec::new();

    let (mut index, build) = measure_phase("build", baseline, config.sample_interval_ms, || {
        build_index(&config)
    })?;
    phases.push(build);

    let (cold_search_ms, cold_search) =
        measure_phase("cold_search", baseline, config.sample_interval_ms, || {
            benchmark_one_search(&config, &index, 0)
        })?;
    phases.push(cold_search);

    let (warm_search, warm_search_phase) =
        measure_phase("warm_search", baseline, config.sample_interval_ms, || {
            benchmark_searches(&config, &index)
        })?;
    phases.push(warm_search_phase);

    let (file_sizes, save) = measure_phase("save", baseline, config.sample_interval_ms, || {
        index
            .save_to_dir_with_options(
                directory.path(),
                IndexPersistenceOptions {
                    include_bm25: config.workload == Workload::Hybrid,
                },
            )
            .map(PersistedFileSizes::from)
            .map_err(MemoryBenchError::from)
    })?;
    phases.push(save);

    let ((), unload) = measure_phase("unload", baseline, config.sample_interval_ms, || {
        drop(index);
        Ok(())
    })?;
    phases.push(unload);

    let (loaded_index, load) = measure_phase("load", baseline, config.sample_interval_ms, || {
        ExactVectorIndex::load_from_dir(directory.path()).map_err(MemoryBenchError::from)
    })?;
    index = loaded_index;
    phases.push(load);

    let (post_load_search, post_load_search_phase) = measure_phase(
        "post_load_search",
        baseline,
        config.sample_interval_ms,
        || benchmark_searches(&config, &index),
    )?;
    phases.push(post_load_search_phase);

    let documents_to_delete =
        ((config.chunks as f64 * config.tombstone_ratio).round() as usize).min(config.chunks);
    let (deleted_chunks, delete) =
        measure_phase("delete", baseline, config.sample_interval_ms, || {
            let mut deleted = 0usize;
            for chunk_id in 0..documents_to_delete {
                deleted += index.delete_document(&document_id(chunk_id));
            }
            Ok(deleted)
        })?;
    phases.push(delete);

    let (compaction, compact) =
        measure_phase("compact", baseline, config.sample_interval_ms, || {
            index.compact().map_err(MemoryBenchError::from)
        })?;
    phases.push(compact);

    let peak_rss_bytes = phases
        .iter()
        .filter_map(|phase| phase.peak_resident_bytes)
        .max();
    let baseline_resident_bytes = baseline.map(|snapshot| snapshot.resident_bytes());
    let peak_delta_bytes = peak_rss_bytes
        .zip(baseline_resident_bytes)
        .map(|(peak, base)| peak.saturating_sub(base));
    let budgets = evaluate_budgets(
        &config.budgets,
        peak_rss_bytes,
        peak_delta_bytes,
        file_sizes.total_bytes,
        post_load_search.p95_ms,
        phases.iter().find(|phase| phase.name == "compact"),
    );

    Ok(MemoryBenchmarkReport {
        schema_version: 1,
        scenario: config,
        platform: PlatformMetadata::current(),
        baseline_memory: baseline,
        peak_rss_bytes,
        peak_delta_bytes,
        phases,
        cold_search_ms,
        warm_search,
        post_load_search,
        persisted_file_sizes: file_sizes,
        deleted_chunks,
        compaction: CompactionSummary::from(compaction),
        budgets,
    })
}

fn build_index(config: &MemoryBenchmarkConfig) -> Result<ExactVectorIndex, MemoryBenchError> {
    let mut index = ExactVectorIndex::try_with_config(
        IndexConfig::new(config.dimension, VectorMetric::Cosine)
            .with_vector_encoding(config.encoding),
    )?;
    for chunk_id in 0..config.chunks {
        index.add_chunk(synthetic_chunk(config, chunk_id))?;
    }
    Ok(index)
}

fn synthetic_chunk(config: &MemoryBenchmarkConfig, chunk_id: usize) -> Chunk {
    Chunk {
        chunk_id: chunk_id as u64,
        document_id: document_id(chunk_id),
        text: format!(
            "synthetic chunk {chunk_id} topic {} local retrieval benchmark",
            chunk_id % 97
        ),
        embedding: normalized_vector(config.dimension, config.seed, chunk_id as u64),
        metadata: Metadata::new(),
        deleted: false,
        version: 1,
    }
}

fn document_id(chunk_id: usize) -> String {
    format!("memory-doc-{chunk_id}")
}

fn benchmark_one_search(
    config: &MemoryBenchmarkConfig,
    index: &ExactVectorIndex,
    query_id: usize,
) -> Result<f64, MemoryBenchError> {
    let query = normalized_vector(
        config.dimension,
        config.seed ^ 0x9e37_79b9_7f4a_7c15,
        query_id as u64,
    );
    let started = Instant::now();
    match config.workload {
        Workload::VectorOnly => {
            black_box(index.search(&SearchQuery::new(query, config.top_k))?);
        }
        Workload::Hybrid => {
            black_box(
                index.hybrid_search(
                    &HybridQuery::new("local retrieval benchmark", query, config.top_k)
                        .with_rrf_k(60.0)
                        .with_candidate_limits(config.vector_candidates, config.keyword_candidates),
                )?,
            );
        }
    }
    Ok(milliseconds(started.elapsed()))
}

fn benchmark_searches(
    config: &MemoryBenchmarkConfig,
    index: &ExactVectorIndex,
) -> Result<SearchLatencyStats, MemoryBenchError> {
    for query_id in 0..config.warmup_queries {
        benchmark_one_search(config, index, query_id)?;
    }
    let mut durations = Vec::with_capacity(config.queries);
    for query_id in 0..config.queries {
        durations.push(benchmark_one_search(
            config,
            index,
            query_id + config.warmup_queries,
        )?);
    }
    durations.sort_by(f64::total_cmp);
    let total = durations.iter().sum::<f64>();
    Ok(SearchLatencyStats {
        samples: durations.len(),
        min_ms: durations.first().copied().unwrap_or(0.0),
        mean_ms: total / durations.len() as f64,
        p50_ms: percentile(&durations, 50),
        p95_ms: percentile(&durations, 95),
        p99_ms: percentile(&durations, 99),
        max_ms: durations.last().copied().unwrap_or(0.0),
    })
}

fn measure_phase<T>(
    name: &'static str,
    baseline: Option<ProcessMemorySnapshot>,
    sample_interval_ms: u64,
    operation: impl FnOnce() -> Result<T, MemoryBenchError>,
) -> Result<(T, PhaseReport), MemoryBenchError> {
    let before = ProcessMemorySnapshot::current();
    let sampler = MemorySampler::start(sample_interval_ms);
    let started = Instant::now();
    let result = operation();
    let elapsed_ms = milliseconds(started.elapsed());
    let after = ProcessMemorySnapshot::current();
    let sampled_peak = sampler.stop();
    let peak_resident_bytes = [
        before.map(|snapshot| snapshot.resident_bytes()),
        after.map(|snapshot| snapshot.resident_bytes()),
        sampled_peak,
    ]
    .into_iter()
    .flatten()
    .max();
    let baseline_bytes = baseline.map(|snapshot| snapshot.resident_bytes());
    let before_bytes = before.map(|snapshot| snapshot.resident_bytes());
    let peak_delta_bytes = peak_resident_bytes
        .zip(baseline_bytes)
        .map(|(peak, base)| peak.saturating_sub(base));
    let peak_increase_bytes = peak_resident_bytes
        .zip(before_bytes)
        .map(|(peak, before)| peak.saturating_sub(before));
    let report = PhaseReport {
        name,
        elapsed_ms,
        memory_before: before,
        memory_after: after,
        peak_resident_bytes,
        peak_delta_bytes,
        peak_increase_bytes,
    };
    result.map(|value| (value, report))
}

struct MemorySampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MemorySampler {
    fn start(sample_interval_ms: u64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let thread_stop = Arc::clone(&stop);
        let thread_peak = Arc::clone(&peak);
        let interval = Duration::from_millis(sample_interval_ms.max(1));
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                if let Some(snapshot) = ProcessMemorySnapshot::current() {
                    thread_peak.fetch_max(snapshot.resident_bytes(), Ordering::Relaxed);
                }
                thread::sleep(interval);
            }
        });
        Self {
            stop,
            peak,
            thread: Some(thread),
        }
    }

    fn stop(mut self) -> Option<u64> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let peak = self.peak.load(Ordering::Relaxed);
        (peak > 0).then_some(peak)
    }
}

fn evaluate_budgets(
    limits: &MemoryBudgets,
    peak_rss_bytes: Option<u64>,
    peak_delta_bytes: Option<u64>,
    persisted_bytes: u64,
    search_p95_ms: f64,
    compact: Option<&PhaseReport>,
) -> BudgetResult {
    let mut violations = Vec::new();
    check_byte_budget(
        &mut violations,
        "peak_rss",
        peak_rss_bytes,
        limits.max_peak_rss_mib,
    );
    check_byte_budget(
        &mut violations,
        "peak_delta",
        peak_delta_bytes,
        limits.max_peak_delta_mib,
    );
    check_byte_budget(
        &mut violations,
        "persisted_size",
        Some(persisted_bytes),
        limits.max_persisted_mib,
    );
    if let Some(limit) = limits.max_search_p95_ms {
        if search_p95_ms > limit {
            violations.push(format!(
                "search_p95_ms measured {search_p95_ms:.3} ms, budget {limit:.3} ms"
            ));
        }
    }
    check_byte_budget(
        &mut violations,
        "compaction_peak_increase",
        compact.and_then(|phase| phase.peak_increase_bytes),
        limits.max_compaction_peak_increase_mib,
    );
    BudgetResult {
        passed: violations.is_empty(),
        violations,
    }
}

fn check_byte_budget(
    violations: &mut Vec<String>,
    label: &str,
    actual: Option<u64>,
    limit_mib: Option<f64>,
) {
    let Some(limit_mib) = limit_mib else { return };
    let Some(actual) = actual else {
        violations.push(format!(
            "{label} budget is configured but resident memory is unavailable on this platform"
        ));
        return;
    };
    let actual_mib = actual as f64 / (1024.0 * 1024.0);
    if actual_mib > limit_mib {
        violations.push(format!(
            "{label} measured {actual_mib:.3} MiB, budget {limit_mib:.3} MiB"
        ));
    }
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    let rank = (sorted.len() * percentile).div_ceil(100);
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

fn normalized_vector(dimension: usize, seed: u64, vector_id: u64) -> Vec<f32> {
    let mut state = seed ^ vector_id.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let mut vector = Vec::with_capacity(dimension);
    for _ in 0..dimension {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let value = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        vector.push(((value >> 40) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0);
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn error_response(message: &str) -> String {
    serde_json::to_string_pretty(&MemoryBenchmarkResponse {
        ok: false,
        report: None,
        error: Some(message.to_owned()),
    })
    .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"failed to serialize error\"}".to_owned())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct MemoryBenchmarkConfig {
    scenario_id: String,
    chunks: usize,
    dimension: usize,
    #[serde(with = "encoding_json")]
    encoding: VectorEncoding,
    workload: Workload,
    queries: usize,
    warmup_queries: usize,
    top_k: usize,
    vector_candidates: usize,
    keyword_candidates: usize,
    tombstone_ratio: f64,
    seed: u64,
    sample_interval_ms: u64,
    budgets: MemoryBudgets,
    environment: EnvironmentMetadata,
}

impl Default for MemoryBenchmarkConfig {
    fn default() -> Self {
        Self {
            scenario_id: "24k-384d-i8-hybrid-t25".to_owned(),
            chunks: 24_000,
            dimension: 384,
            encoding: VectorEncoding::I8ScalarQuantized,
            workload: Workload::Hybrid,
            queries: 50,
            warmup_queries: 3,
            top_k: 10,
            vector_candidates: 50,
            keyword_candidates: 50,
            tombstone_ratio: 0.25,
            seed: 42,
            sample_interval_ms: 1,
            budgets: MemoryBudgets::default(),
            environment: EnvironmentMetadata::default(),
        }
    }
}

impl MemoryBenchmarkConfig {
    fn validate(&self) -> Result<(), MemoryBenchError> {
        if self.scenario_id.trim().is_empty() {
            return Err(MemoryBenchError::InvalidConfig(
                "scenario_id cannot be empty".to_owned(),
            ));
        }
        if self.chunks == 0 || self.dimension == 0 || self.queries == 0 || self.top_k == 0 {
            return Err(MemoryBenchError::InvalidConfig(
                "chunks, dimension, queries, and top_k must be greater than zero".to_owned(),
            ));
        }
        if self.vector_candidates < self.top_k || self.keyword_candidates < self.top_k {
            return Err(MemoryBenchError::InvalidConfig(
                "candidate limits must be greater than or equal to top_k".to_owned(),
            ));
        }
        if !(0.0..=1.0).contains(&self.tombstone_ratio) {
            return Err(MemoryBenchError::InvalidConfig(
                "tombstone_ratio must be between 0.0 and 1.0".to_owned(),
            ));
        }
        if self.sample_interval_ms == 0 {
            return Err(MemoryBenchError::InvalidConfig(
                "sample_interval_ms must be greater than zero".to_owned(),
            ));
        }
        self.budgets.validate()
    }
}

mod encoding_json {
    use serde::{Deserialize, Deserializer, Serializer};
    use vectorkit_core::VectorEncoding;

    pub fn serialize<S>(encoding: &VectorEncoding, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match encoding {
            VectorEncoding::F32 => "f32",
            VectorEncoding::F16 => "f16",
            VectorEncoding::BF16 => "bf16",
            VectorEncoding::I8ScalarQuantized => "i8",
            VectorEncoding::BinaryQuantized => "binary",
        })
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VectorEncoding, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "f32" => Ok(VectorEncoding::F32),
            "f16" => Ok(VectorEncoding::F16),
            "i8" | "i8-scalar-quantized" => Ok(VectorEncoding::I8ScalarQuantized),
            _ => Err(serde::de::Error::custom("encoding must be f32, f16, or i8")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Workload {
    VectorOnly,
    #[default]
    Hybrid,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct MemoryBudgets {
    max_peak_rss_mib: Option<f64>,
    max_peak_delta_mib: Option<f64>,
    max_persisted_mib: Option<f64>,
    max_search_p95_ms: Option<f64>,
    max_compaction_peak_increase_mib: Option<f64>,
}

impl MemoryBudgets {
    fn validate(&self) -> Result<(), MemoryBenchError> {
        for (name, value) in [
            ("max_peak_rss_mib", self.max_peak_rss_mib),
            ("max_peak_delta_mib", self.max_peak_delta_mib),
            ("max_persisted_mib", self.max_persisted_mib),
            ("max_search_p95_ms", self.max_search_p95_ms),
            (
                "max_compaction_peak_increase_mib",
                self.max_compaction_peak_increase_mib,
            ),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
                return Err(MemoryBenchError::InvalidConfig(format!(
                    "{name} must be a positive finite number"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct EnvironmentMetadata {
    device_model: Option<String>,
    os_version: Option<String>,
    build_configuration: Option<String>,
    app_version: Option<String>,
}

#[derive(Debug, Serialize)]
struct MemoryBenchmarkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<MemoryBenchmarkReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct MemoryBenchmarkReport {
    schema_version: u32,
    scenario: MemoryBenchmarkConfig,
    platform: PlatformMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_memory: Option<ProcessMemorySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_rss_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_delta_bytes: Option<u64>,
    phases: Vec<PhaseReport>,
    cold_search_ms: f64,
    warm_search: SearchLatencyStats,
    post_load_search: SearchLatencyStats,
    persisted_file_sizes: PersistedFileSizes,
    deleted_chunks: usize,
    compaction: CompactionSummary,
    budgets: BudgetResult,
}

#[derive(Debug, Serialize)]
struct PlatformMetadata {
    os: &'static str,
    architecture: &'static str,
    debug_assertions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    machine_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_release: Option<String>,
}

impl PlatformMetadata {
    fn current() -> Self {
        Self {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            debug_assertions: cfg!(debug_assertions),
            machine_identifier: platform_uname_field(UnameField::Machine),
            os_release: platform_uname_field(UnameField::Release),
        }
    }
}

#[derive(Clone, Copy)]
enum UnameField {
    Machine,
    Release,
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn platform_uname_field(field: UnameField) -> Option<String> {
    let mut name = std::mem::MaybeUninit::<libc::utsname>::uninit();
    if unsafe { libc::uname(name.as_mut_ptr()) } != 0 {
        return None;
    }
    let name = unsafe { name.assume_init() };
    let value = match field {
        UnameField::Machine => name.machine.as_ptr(),
        UnameField::Release => name.release.as_ptr(),
    };
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .ok()
        .map(ToOwned::to_owned)
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn platform_uname_field(_field: UnameField) -> Option<String> {
    None
}

#[derive(Debug, Serialize)]
struct PhaseReport {
    name: &'static str,
    elapsed_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_before: Option<ProcessMemorySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_after: Option<ProcessMemorySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_resident_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_delta_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_increase_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct SearchLatencyStats {
    samples: usize,
    min_ms: f64,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Serialize)]
struct PersistedFileSizes {
    total_bytes: u64,
    manifest_bytes: u64,
    vectors_bytes: u64,
    chunks_bytes: u64,
    records_bytes: u64,
    bm25_bytes: u64,
    tombstones_bytes: u64,
}

impl From<IndexFileSizeReport> for PersistedFileSizes {
    fn from(value: IndexFileSizeReport) -> Self {
        Self {
            total_bytes: value.total_bytes(),
            manifest_bytes: value.manifest_bytes,
            vectors_bytes: value.vectors_bytes,
            chunks_bytes: value.chunks_bytes,
            records_bytes: value.records_bytes,
            bm25_bytes: value.bm25_bytes,
            tombstones_bytes: value.tombstones_bytes,
        }
    }
}

#[derive(Debug, Serialize)]
struct CompactionSummary {
    chunks_before: usize,
    chunks_after: usize,
    chunks_removed: usize,
    estimated_bytes_before: usize,
    estimated_bytes_after: usize,
    estimated_bytes_reclaimed: usize,
}

impl From<CompactionReport> for CompactionSummary {
    fn from(value: CompactionReport) -> Self {
        Self {
            chunks_before: value.chunks_before,
            chunks_after: value.chunks_after,
            chunks_removed: value.chunks_removed,
            estimated_bytes_before: value.estimated_bytes_before,
            estimated_bytes_after: value.estimated_bytes_after,
            estimated_bytes_reclaimed: value.estimated_bytes_reclaimed,
        }
    }
}

#[derive(Debug, Serialize)]
struct BudgetResult {
    passed: bool,
    violations: Vec<String>,
}

#[derive(Debug)]
enum MemoryBenchError {
    InvalidConfig(String),
    Io(io::Error),
    Json(serde_json::Error),
    Core(vectorkit_core::VectorKitError),
}

impl Display for MemoryBenchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => formatter.write_str(message),
            Self::Io(error) => Display::fmt(error, formatter),
            Self::Json(error) => Display::fmt(error, formatter),
            Self::Core(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for MemoryBenchError {}

impl From<io::Error> for MemoryBenchError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for MemoryBenchError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<vectorkit_core::VectorKitError> for MemoryBenchError {
    fn from(value: vectorkit_core::VectorKitError) -> Self {
        Self::Core(value)
    }
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(scenario_id: &str) -> Result<Self, io::Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let safe_id = scenario_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let path = std::env::temp_dir().join(format!(
            "vectorkit-memory-{safe_id}-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_scenario_reports_every_phase_and_compaction() {
        let config = r#"{
            "scenario_id":"test",
            "chunks":32,
            "dimension":8,
            "encoding":"i8",
            "workload":"hybrid",
            "queries":3,
            "warmup_queries":1,
            "top_k":2,
            "vector_candidates":4,
            "keyword_candidates":4,
            "tombstone_ratio":0.25
        }"#;
        let (json, passed) = memory_benchmark_json(config);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(passed);
        assert_eq!(value["ok"], true);
        assert_eq!(value["report"]["deleted_chunks"], 8);
        assert_eq!(value["report"]["compaction"]["chunks_removed"], 8);
        assert_eq!(value["report"]["phases"].as_array().unwrap().len(), 9);
    }

    #[test]
    fn configured_budget_failure_is_machine_readable() {
        let config = r#"{
            "scenario_id":"budget-test",
            "chunks":8,
            "dimension":4,
            "encoding":"f32",
            "queries":1,
            "top_k":1,
            "vector_candidates":1,
            "keyword_candidates":1,
            "budgets":{"max_persisted_mib":0.000001}
        }"#;
        let (json, passed) = memory_benchmark_json(config);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(!passed);
        assert_eq!(value["ok"], true);
        assert_eq!(value["report"]["budgets"]["passed"], false);
        assert!(!value["report"]["budgets"]["violations"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rejects_multi_dimension_config_shape() {
        let (json, passed) = memory_benchmark_json(r#"{"dimensions":[8,16]}"#);
        assert!(!passed);
        assert!(json.contains("unknown field") || json.contains("dimension"));
    }
}
