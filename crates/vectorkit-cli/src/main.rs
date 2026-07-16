use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use simsimd::{bf16, capabilities, f16, SpatialSimilarity};
use vectorkit_core::{
    diagnostic_dot_product_i8, Chunk, ExactVectorIndex, Filter, HybridQuery, IndexConfig,
    IndexFileSizeReport, IndexSizeEstimate, KeywordQuery, Metadata, MetadataValue, SearchQuery,
    VectorEncoding, VectorMetric,
};
use vectorkit_ffi::memory_benchmark_json;

mod quality;

const BENCH_FILTER_FIELD: &str = "__bench_filter_bucket";

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), CliError> {
    match args.as_slice() {
        [command, subcommand, rest @ ..] if command == "bench" && subcommand == "synthetic" => {
            run_synthetic_bench(SyntheticBenchConfig::parse(rest)?)
        }
        [command, subcommand, rest @ ..] if command == "bench" && subcommand == "matrix" => {
            run_matrix_bench(MatrixBenchConfig::parse(rest)?)
        }
        [command, subcommand, rest @ ..] if command == "bench" && subcommand == "kernels" => {
            run_kernel_bench(KernelBenchConfig::parse(rest)?)
        }
        [command, subcommand, rest @ ..] if command == "bench" && subcommand == "topk" => {
            run_topk_bench(TopKBenchConfig::parse(rest)?)
        }
        [command, subcommand, rest @ ..] if command == "bench" && subcommand == "memory" => {
            run_memory_bench(rest)
        }
        [command, subcommand, rest @ ..] if command == "bench" && subcommand == "quality" => {
            run_quality_bench(rest)
        }
        _ => Err(CliError::usage()),
    }
}

fn run_quality_bench(args: &[String]) -> Result<(), CliError> {
    let outcome = quality::run(args).map_err(CliError::InvalidArgument)?;
    println!("{}", outcome.json);
    if outcome.passed {
        Ok(())
    } else {
        Err(CliError::InvalidArgument(
            "retrieval-quality benchmark failed a configured quality gate".to_owned(),
        ))
    }
}

fn run_memory_bench(args: &[String]) -> Result<(), CliError> {
    let config_json =
        match args {
            [] => String::new(),
            [flag, path] if flag == "--config" => fs::read_to_string(path).map_err(|error| {
                CliError::InvalidArgument(format!("failed to read memory config '{path}': {error}"))
            })?,
            [flag, json] if flag == "--config-json" => json.clone(),
            _ => return Err(CliError::InvalidArgument(
                "usage: vectorkit bench memory [--config <scenario.json> | --config-json <json>]"
                    .to_owned(),
            )),
        };
    let (json, passed) = memory_benchmark_json(&config_json);
    println!("{json}");
    if passed {
        Ok(())
    } else {
        Err(CliError::InvalidArgument(
            "memory benchmark failed or exceeded a configured budget".to_owned(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SyntheticBenchConfig {
    chunks: usize,
    dimension: usize,
    queries: usize,
    top_k: usize,
    vector_candidates: usize,
    keyword_candidates: usize,
    search_mode: SearchMode,
    encoding: VectorEncoding,
    metric: VectorMetric,
    seed: u64,
    filter_every: Option<usize>,
    persist_dir: Option<PathBuf>,
    footprint: FootprintConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    Vector,
    Keyword,
    HybridWeighted,
    HybridRrf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FootprintConfig {
    budget_bytes: usize,
    avg_chunk_data_bytes: usize,
    avg_metadata_bytes: usize,
    avg_bm25_terms_per_chunk: usize,
}

impl Default for SyntheticBenchConfig {
    fn default() -> Self {
        Self {
            chunks: 1_000,
            dimension: 384,
            queries: 100,
            top_k: 10,
            vector_candidates: 50,
            keyword_candidates: 50,
            search_mode: SearchMode::Vector,
            encoding: VectorEncoding::F32,
            metric: VectorMetric::Cosine,
            seed: 42,
            filter_every: None,
            persist_dir: None,
            footprint: FootprintConfig::default(),
        }
    }
}

impl Default for FootprintConfig {
    fn default() -> Self {
        Self {
            budget_bytes: 20 * 1024 * 1024,
            avg_chunk_data_bytes: 256,
            avg_metadata_bytes: 32,
            avg_bm25_terms_per_chunk: 24,
        }
    }
}

impl SyntheticBenchConfig {
    fn parse(args: &[String]) -> Result<Self, CliError> {
        let mut config = Self::default();
        let mut index = 0;

        while index < args.len() {
            let flag = args[index].as_str();
            let Some(value) = args.get(index + 1) else {
                return Err(CliError::InvalidArgument(format!(
                    "missing value for argument '{flag}'"
                )));
            };

            match flag {
                "--chunks" => config.chunks = parse_positive(value, flag)?,
                "--dimension" => config.dimension = parse_positive(value, flag)?,
                "--queries" => config.queries = parse_positive(value, flag)?,
                "--top-k" => config.top_k = parse_positive(value, flag)?,
                "--vector-candidates" => config.vector_candidates = parse_positive(value, flag)?,
                "--keyword-candidates" => config.keyword_candidates = parse_positive(value, flag)?,
                "--search-mode" => config.search_mode = parse_search_mode(value)?,
                "--encoding" => config.encoding = parse_encoding(value)?,
                "--metric" => config.metric = parse_metric(value)?,
                "--seed" => config.seed = parse_u64(value, flag)?,
                "--filter-every" => config.filter_every = Some(parse_positive(value, flag)?),
                "--persist-dir" => config.persist_dir = Some(PathBuf::from(value)),
                "--budget-mb" => {
                    config.footprint.budget_bytes = parse_mib(value, flag)?;
                }
                "--avg-chunk-data-bytes" => {
                    config.footprint.avg_chunk_data_bytes = parse_nonnegative(value, flag)?;
                }
                "--avg-metadata-bytes" => {
                    config.footprint.avg_metadata_bytes = parse_nonnegative(value, flag)?;
                }
                "--avg-bm25-terms" => {
                    config.footprint.avg_bm25_terms_per_chunk = parse_nonnegative(value, flag)?;
                }
                "--help" | "-h" => return Err(CliError::usage()),
                _ => {
                    return Err(CliError::InvalidArgument(format!(
                        "unknown argument '{flag}'"
                    )));
                }
            }

            index += 2;
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MatrixBenchConfig {
    chunks: usize,
    dimensions: Vec<usize>,
    queries: usize,
    top_ks: Vec<usize>,
    vector_candidate_limits: Vec<usize>,
    keyword_candidate_limits: Vec<usize>,
    search_modes: Vec<SearchMode>,
    encodings: Vec<VectorEncoding>,
    metric: VectorMetric,
    seed: u64,
    filter_every_values: Vec<Option<usize>>,
    persist_dir: Option<PathBuf>,
    footprint: FootprintConfig,
}

impl Default for MatrixBenchConfig {
    fn default() -> Self {
        Self {
            chunks: 1_000,
            dimensions: vec![384, 768, 1536],
            queries: 100,
            top_ks: vec![5, 10],
            vector_candidate_limits: vec![50],
            keyword_candidate_limits: vec![50],
            search_modes: vec![SearchMode::Vector],
            encodings: vec![
                VectorEncoding::F32,
                VectorEncoding::F16,
                VectorEncoding::BF16,
                VectorEncoding::I8ScalarQuantized,
            ],
            metric: VectorMetric::Cosine,
            seed: 42,
            filter_every_values: vec![None],
            persist_dir: None,
            footprint: FootprintConfig::default(),
        }
    }
}

impl MatrixBenchConfig {
    fn parse(args: &[String]) -> Result<Self, CliError> {
        let mut config = Self::default();
        let mut index = 0;

        while index < args.len() {
            let flag = args[index].as_str();
            let Some(value) = args.get(index + 1) else {
                return Err(CliError::InvalidArgument(format!(
                    "missing value for argument '{flag}'"
                )));
            };

            match flag {
                "--chunks" => config.chunks = parse_positive(value, flag)?,
                "--dimensions" => config.dimensions = parse_positive_list(value, flag)?,
                "--queries" => config.queries = parse_positive(value, flag)?,
                "--top-k" => config.top_ks = parse_positive_list(value, flag)?,
                "--vector-candidates" => {
                    config.vector_candidate_limits = parse_positive_list(value, flag)?
                }
                "--keyword-candidates" => {
                    config.keyword_candidate_limits = parse_positive_list(value, flag)?
                }
                "--search-modes" => config.search_modes = parse_search_mode_list(value)?,
                "--encodings" => config.encodings = parse_encoding_list(value)?,
                "--metric" => config.metric = parse_metric(value)?,
                "--seed" => config.seed = parse_u64(value, flag)?,
                "--filter-every" => {
                    config.filter_every_values = vec![Some(parse_positive(value, flag)?)]
                }
                "--filter-every-values" => {
                    config.filter_every_values = parse_filter_every_list(value, flag)?
                }
                "--persist-dir" => config.persist_dir = Some(PathBuf::from(value)),
                "--budget-mb" => {
                    config.footprint.budget_bytes = parse_mib(value, flag)?;
                }
                "--avg-chunk-data-bytes" => {
                    config.footprint.avg_chunk_data_bytes = parse_nonnegative(value, flag)?;
                }
                "--avg-metadata-bytes" => {
                    config.footprint.avg_metadata_bytes = parse_nonnegative(value, flag)?;
                }
                "--avg-bm25-terms" => {
                    config.footprint.avg_bm25_terms_per_chunk = parse_nonnegative(value, flag)?;
                }
                "--help" | "-h" => return Err(CliError::usage()),
                _ => {
                    return Err(CliError::InvalidArgument(format!(
                        "unknown argument '{flag}'"
                    )));
                }
            }

            index += 2;
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct KernelBenchConfig {
    vectors: usize,
    dimensions: Vec<usize>,
    queries: usize,
    encodings: Vec<VectorEncoding>,
    seed: u64,
}

impl Default for KernelBenchConfig {
    fn default() -> Self {
        Self {
            vectors: 24_000,
            dimensions: vec![384, 768],
            queries: 200,
            encodings: vec![
                VectorEncoding::F32,
                VectorEncoding::F16,
                VectorEncoding::I8ScalarQuantized,
            ],
            seed: 42,
        }
    }
}

impl KernelBenchConfig {
    fn parse(args: &[String]) -> Result<Self, CliError> {
        let mut config = Self::default();
        let mut index = 0;

        while index < args.len() {
            let flag = args[index].as_str();
            let Some(value) = args.get(index + 1) else {
                return Err(CliError::InvalidArgument(format!(
                    "missing value for argument '{flag}'"
                )));
            };

            match flag {
                "--vectors" => config.vectors = parse_positive(value, flag)?,
                "--dimensions" => config.dimensions = parse_positive_list(value, flag)?,
                "--queries" => config.queries = parse_positive(value, flag)?,
                "--encodings" => config.encodings = parse_encoding_list(value)?,
                "--seed" => config.seed = parse_u64(value, flag)?,
                "--help" | "-h" => return Err(CliError::usage()),
                _ => {
                    return Err(CliError::InvalidArgument(format!(
                        "unknown argument '{flag}'"
                    )));
                }
            }

            index += 2;
        }

        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TopKBenchConfig {
    candidates: usize,
    queries: usize,
    top_ks: Vec<usize>,
    seed: u64,
}

impl Default for TopKBenchConfig {
    fn default() -> Self {
        Self {
            candidates: 50_000,
            queries: 1_000,
            top_ks: vec![5, 10, 50, 100],
            seed: 42,
        }
    }
}

impl TopKBenchConfig {
    fn parse(args: &[String]) -> Result<Self, CliError> {
        let mut config = Self::default();
        let mut index = 0;

        while index < args.len() {
            let flag = args[index].as_str();
            let Some(value) = args.get(index + 1) else {
                return Err(CliError::InvalidArgument(format!(
                    "missing value for argument '{flag}'"
                )));
            };

            match flag {
                "--candidates" => config.candidates = parse_positive(value, flag)?,
                "--queries" => config.queries = parse_positive(value, flag)?,
                "--top-k" => config.top_ks = parse_positive_list(value, flag)?,
                "--seed" => config.seed = parse_u64(value, flag)?,
                "--help" | "-h" => return Err(CliError::usage()),
                _ => {
                    return Err(CliError::InvalidArgument(format!(
                        "unknown argument '{flag}'"
                    )));
                }
            }

            index += 2;
        }

        Ok(config)
    }
}

#[derive(Debug, Clone)]
struct SyntheticBenchReport {
    config: SyntheticBenchConfig,
    footprint: FootprintEstimate,
    index_size: IndexSizeEstimate,
    persisted_file_sizes: Option<IndexFileSizeReport>,
    source_embedding_bytes: usize,
    build_duration: Duration,
    vector_candidate_avg: Option<Duration>,
    keyword_candidate_avg: Option<Duration>,
    query_min: Duration,
    query_avg: Duration,
    query_p50: Duration,
    query_p95: Duration,
    query_max: Duration,
    recall_at_k_vs_f32: f64,
    total_hits: usize,
    top_hit_checksum: u64,
}

#[derive(Debug, Clone)]
struct KernelBenchReport {
    vectors: usize,
    dimension: usize,
    encoding: VectorEncoding,
    payload_bytes: usize,
    query_min: Duration,
    query_avg: Duration,
    query_p50: Duration,
    query_p95: Duration,
    query_max: Duration,
    score_checksum: f64,
}

#[derive(Debug, Clone)]
struct TopKBenchReport {
    top_k: usize,
    algorithm: TopKAlgorithm,
    query_min: Duration,
    query_avg: Duration,
    query_p50: Duration,
    query_p95: Duration,
    query_max: Duration,
    checksum: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopKAlgorithm {
    BoundedVec,
    BinaryHeap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FootprintEstimate {
    vector_bytes: usize,
    chunk_fixed_bytes: usize,
    chunk_data_bytes: usize,
    metadata_bytes: usize,
    bm25_bytes: usize,
    overhead_bytes: usize,
}

impl FootprintEstimate {
    fn auxiliary_bytes(&self) -> usize {
        self.chunk_fixed_bytes
            + self.chunk_data_bytes
            + self.metadata_bytes
            + self.bm25_bytes
            + self.overhead_bytes
    }

    fn total_bytes(&self) -> usize {
        self.vector_bytes + self.auxiliary_bytes()
    }

    fn budget_headroom_bytes(&self, budget_bytes: usize) -> isize {
        budget_bytes as isize - self.total_bytes() as isize
    }
}

fn run_synthetic_bench(config: SyntheticBenchConfig) -> Result<(), CliError> {
    let report = benchmark_synthetic(config)?;

    println!("VectorKit synthetic benchmark");
    println!("chunks: {}", report.config.chunks);
    println!("dimension: {}", report.config.dimension);
    println!("queries: {}", report.config.queries);
    println!("top_k: {}", report.config.top_k);
    println!("vector_candidates: {}", report.config.vector_candidates);
    println!("keyword_candidates: {}", report.config.keyword_candidates);
    println!(
        "search_mode: {}",
        search_mode_name(report.config.search_mode)
    );
    println!("encoding: {}", encoding_name(report.config.encoding));
    println!("metric: {}", metric_name(report.config.metric));
    println!("seed: {}", report.config.seed);
    println!(
        "filter_every: {}",
        filter_every_name(report.config.filter_every)
    );
    println!("vector_mb: {:.3}", mib(report.footprint.vector_bytes));
    println!(
        "estimated_auxiliary_mb: {:.3}",
        mib(report.footprint.auxiliary_bytes())
    );
    println!(
        "estimated_total_index_mb: {:.3}",
        mib(report.footprint.total_bytes())
    );
    println!(
        "budget_headroom_mb: {:.3}",
        signed_mib(
            report
                .footprint
                .budget_headroom_bytes(report.config.footprint.budget_bytes)
        )
    );
    println!(
        "budget_mb: {:.3}",
        mib(report.config.footprint.budget_bytes)
    );
    println!(
        "avg_chunk_data_bytes: {}",
        report.config.footprint.avg_chunk_data_bytes
    );
    println!(
        "avg_metadata_bytes: {}",
        report.config.footprint.avg_metadata_bytes
    );
    println!(
        "avg_bm25_terms_per_chunk: {}",
        report.config.footprint.avg_bm25_terms_per_chunk
    );
    println!(
        "retained_source_f32_mb: {:.3}",
        mib(report.source_embedding_bytes)
    );
    println!(
        "total_vector_mb_current: {:.3}",
        mib(report.footprint.vector_bytes + report.source_embedding_bytes)
    );
    println!(
        "current_index_payload_mb: {:.3}",
        mib(report.index_size.total_bytes())
    );
    println!(
        "current_vector_payload_mb: {:.3}",
        mib(report.index_size.vector_bytes)
    );
    println!(
        "current_chunk_payload_mb: {:.3}",
        mib(report.index_size.chunk_bytes())
    );
    println!(
        "current_bm25_payload_mb: {:.3}",
        mib(report.index_size.bm25_bytes)
    );
    println!(
        "current_metadata_filter_payload_mb: {:.3}",
        mib(report.index_size.metadata_filter_bytes)
    );
    println!(
        "current_record_store_payload_mb: {:.3}",
        mib(report.index_size.record_store_bytes)
    );
    println!(
        "current_chunk_identity_payload_mb: {:.3}",
        mib(report.index_size.chunk_identity_bytes)
    );
    if let Some(file_sizes) = report.persisted_file_sizes {
        println!(
            "persisted_total_index_mb: {:.3}",
            mib_u64(file_sizes.total_bytes())
        );
        println!(
            "persisted_manifest_mb: {:.3}",
            mib_u64(file_sizes.manifest_bytes)
        );
        println!(
            "persisted_vectors_mb: {:.3}",
            mib_u64(file_sizes.vectors_bytes)
        );
        println!(
            "persisted_chunks_mb: {:.3}",
            mib_u64(file_sizes.chunks_bytes)
        );
        println!(
            "persisted_records_mb: {:.3}",
            mib_u64(file_sizes.records_bytes)
        );
        println!("persisted_bm25_mb: {:.3}", mib_u64(file_sizes.bm25_bytes));
        println!(
            "persisted_tombstones_mb: {:.3}",
            mib_u64(file_sizes.tombstones_bytes)
        );
    }
    println!("build_ms: {:.3}", millis(report.build_duration));
    if let Some(duration) = report.vector_candidate_avg {
        println!("vector_candidate_avg_ms: {:.3}", millis(duration));
    }
    if let Some(duration) = report.keyword_candidate_avg {
        println!("keyword_candidate_avg_ms: {:.3}", millis(duration));
    }
    println!("query_min_ms: {:.3}", millis(report.query_min));
    println!("query_avg_ms: {:.3}", millis(report.query_avg));
    println!("query_p50_ms: {:.3}", millis(report.query_p50));
    println!("query_p95_ms: {:.3}", millis(report.query_p95));
    println!("query_max_ms: {:.3}", millis(report.query_max));
    println!("recall_at_k_vs_f32: {:.4}", report.recall_at_k_vs_f32);
    println!("total_hits: {}", report.total_hits);
    println!("top_hit_checksum: {}", report.top_hit_checksum);

    Ok(())
}

fn run_matrix_bench(config: MatrixBenchConfig) -> Result<(), CliError> {
    println!(
        "| chunks | dim | top_k | vector candidates | keyword candidates | mode | enc | metric | filter every | vector MB | aux MB | est total MB | current payload MB | persisted MB | headroom MB | retained f32 MB | build ms | vector cand avg ms | keyword cand avg ms | min ms | avg ms | p50 ms | p95 ms | max ms | recall@k vs f32 | hits | checksum |"
    );
    println!(
        "|---:|---:|---:|:---|:---|:---|:---|:---|:---|---:|---:|---:|---:|:---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    );

    for dimension in &config.dimensions {
        for top_k in &config.top_ks {
            for search_mode in &config.search_modes {
                for encoding in &config.encodings {
                    for (vector_candidates, keyword_candidates) in candidate_limit_pairs(
                        *search_mode,
                        &config.vector_candidate_limits,
                        &config.keyword_candidate_limits,
                    ) {
                        for filter_every in &config.filter_every_values {
                            let persist_dir = config.persist_dir.as_ref().map(|base| {
                                base.join(format!(
                                    "chunks-{}-dim-{}-topk-{}-{}-{}-vcand-{}-kcand-{}-filter-{}",
                                    config.chunks,
                                    dimension,
                                    top_k,
                                    search_mode_name(*search_mode),
                                    encoding_name(*encoding),
                                    vector_candidates,
                                    keyword_candidates,
                                    filter_every_name(*filter_every)
                                ))
                            });
                            let report = benchmark_synthetic(SyntheticBenchConfig {
                                chunks: config.chunks,
                                dimension: *dimension,
                                queries: config.queries,
                                top_k: *top_k,
                                vector_candidates,
                                keyword_candidates,
                                search_mode: *search_mode,
                                encoding: *encoding,
                                metric: config.metric,
                                seed: config.seed,
                                filter_every: *filter_every,
                                persist_dir,
                                footprint: config.footprint.clone(),
                            })?;

                            println!(
                                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {:.3} | {:.3} | {:.3} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.4} | {} | {} |",
                                report.config.chunks,
                                report.config.dimension,
                                report.config.top_k,
                                hybrid_candidate_limit_cell(
                                    report.config.search_mode,
                                    report.config.vector_candidates
                                ),
                                hybrid_candidate_limit_cell(
                                    report.config.search_mode,
                                    report.config.keyword_candidates
                                ),
                                search_mode_name(report.config.search_mode),
                                encoding_name(report.config.encoding),
                                metric_name(report.config.metric),
                                filter_every_name(report.config.filter_every),
                                mib(report.footprint.vector_bytes),
                                mib(report.footprint.auxiliary_bytes()),
                                mib(report.footprint.total_bytes()),
                                mib(report.index_size.total_bytes()),
                                persisted_mb_cell(report.persisted_file_sizes),
                                signed_mib(
                                    report
                                        .footprint
                                        .budget_headroom_bytes(report.config.footprint.budget_bytes)
                                ),
                                mib(report.source_embedding_bytes),
                                millis(report.build_duration),
                                optional_millis_cell(report.vector_candidate_avg),
                                optional_millis_cell(report.keyword_candidate_avg),
                                millis(report.query_min),
                                millis(report.query_avg),
                                millis(report.query_p50),
                                millis(report.query_p95),
                                millis(report.query_max),
                                report.recall_at_k_vs_f32,
                                report.total_hits,
                                report.top_hit_checksum,
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn candidate_limit_pairs(
    search_mode: SearchMode,
    vector_candidate_limits: &[usize],
    keyword_candidate_limits: &[usize],
) -> Vec<(usize, usize)> {
    if !is_hybrid_mode(search_mode) {
        return vec![(
            first_candidate_limit(vector_candidate_limits),
            first_candidate_limit(keyword_candidate_limits),
        )];
    }

    let mut pairs =
        Vec::with_capacity(vector_candidate_limits.len() * keyword_candidate_limits.len());
    for vector_candidates in vector_candidate_limits {
        for keyword_candidates in keyword_candidate_limits {
            pairs.push((*vector_candidates, *keyword_candidates));
        }
    }
    pairs
}

fn first_candidate_limit(candidate_limits: &[usize]) -> usize {
    candidate_limits.first().copied().unwrap_or(50)
}

fn is_hybrid_mode(search_mode: SearchMode) -> bool {
    matches!(
        search_mode,
        SearchMode::HybridWeighted | SearchMode::HybridRrf
    )
}

fn run_kernel_bench(config: KernelBenchConfig) -> Result<(), CliError> {
    println!("VectorKit scoring kernel benchmark");
    println!("vectors: {}", config.vectors);
    println!("queries: {}", config.queries);
    println!("seed: {}", config.seed);
    println!("simsimd_capabilities: {}", simsimd_capability_summary());
    println!(
        "| vectors | dim | enc | payload MB | min ms | avg ms | p50 ms | p95 ms | max ms | score checksum |"
    );
    println!("|---:|---:|:---|---:|---:|---:|---:|---:|---:|---:|");

    for dimension in &config.dimensions {
        for encoding in &config.encodings {
            let report = benchmark_kernel(&config, *dimension, *encoding);
            println!(
                "| {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
                report.vectors,
                report.dimension,
                encoding_name(report.encoding),
                mib(report.payload_bytes),
                millis(report.query_min),
                millis(report.query_avg),
                millis(report.query_p50),
                millis(report.query_p95),
                millis(report.query_max),
                report.score_checksum,
            );
        }
    }

    Ok(())
}

fn run_topk_bench(config: TopKBenchConfig) -> Result<(), CliError> {
    println!("VectorKit top-k maintenance benchmark");
    println!("candidates/query: {}", config.candidates);
    println!("queries: {}", config.queries);
    println!("seed: {}", config.seed);
    println!("| k | algorithm | min ms | avg ms | p50 ms | p95 ms | max ms | checksum |");
    println!("|---:|:---|---:|---:|---:|---:|---:|---:|");

    let mut accumulators = config
        .top_ks
        .iter()
        .copied()
        .map(TopKBenchAccumulator::new)
        .collect::<Vec<_>>();

    for query_id in 0..config.queries {
        let candidates = topk_candidates(config.candidates, config.seed, query_id);
        for accumulator in &mut accumulators {
            accumulator.record(&candidates);
        }
    }

    for accumulator in accumulators {
        for report in accumulator.reports() {
            println!(
                "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} |",
                report.top_k,
                topk_algorithm_name(report.algorithm),
                millis(report.query_min),
                millis(report.query_avg),
                millis(report.query_p50),
                millis(report.query_p95),
                millis(report.query_max),
                report.checksum,
            );
        }
    }

    Ok(())
}

fn benchmark_synthetic(config: SyntheticBenchConfig) -> Result<SyntheticBenchReport, CliError> {
    let (index, build_duration) = build_synthetic_index(&config, config.encoding)?;
    let index_size = index.size_estimate();
    let persisted_file_sizes = config
        .persist_dir
        .as_ref()
        .map(|directory| index.save_to_dir(directory))
        .transpose()?;
    let f32_ground_truth = if config.encoding == VectorEncoding::F32 {
        None
    } else {
        Some(build_synthetic_index(&config, VectorEncoding::F32)?.0)
    };

    let mut query_durations = Vec::with_capacity(config.queries);
    let mut vector_candidate_durations = Vec::new();
    let mut keyword_candidate_durations = Vec::new();
    let mut recall_sum = 0.0;
    let mut total_hits = 0usize;
    let mut top_hit_checksum = 0u64;

    for query_id in 0..config.queries {
        let target_chunk = target_chunk_id(query_id, config.chunks);
        let query =
            generate_query_vector(config.dimension, config.seed, target_chunk as u64, query_id);
        let query_text = synthetic_query_text(target_chunk);
        let search_spec = SyntheticSearchSpec {
            top_k: config.top_k,
            vector_candidates: config.vector_candidates,
            keyword_candidates: config.keyword_candidates,
            filter_every: config.filter_every,
            target_chunk,
        };

        if matches!(
            config.search_mode,
            SearchMode::HybridWeighted | SearchMode::HybridRrf
        ) {
            let search_query = synthetic_search_query(
                search_spec.vector_candidates,
                query.clone(),
                search_spec.filter_every,
                search_spec.target_chunk,
            );
            let vector_start = Instant::now();
            let vector_hits = index.search(&search_query)?;
            vector_candidate_durations.push(vector_start.elapsed());
            black_box(vector_hits.len());

            let keyword_query = synthetic_keyword_query(
                search_spec.keyword_candidates,
                &query_text,
                search_spec.filter_every,
                search_spec.target_chunk,
            );
            let keyword_start = Instant::now();
            let keyword_hits = index.keyword_search(&keyword_query)?;
            keyword_candidate_durations.push(keyword_start.elapsed());
            black_box(keyword_hits.len());
        }

        let start = Instant::now();
        let hits = run_synthetic_search_mode(
            &index,
            config.search_mode,
            search_spec,
            query.clone(),
            &query_text,
        )?;
        query_durations.push(start.elapsed());

        recall_sum += match &f32_ground_truth {
            Some(ground_truth) => {
                let ground_truth_hits = run_synthetic_search_mode(
                    ground_truth,
                    config.search_mode,
                    search_spec,
                    query,
                    &query_text,
                )?;
                recall_at_k(&hits, &ground_truth_hits)
            }
            None => 1.0,
        };

        total_hits += hits.len();
        if let Some(hit) = hits.first() {
            top_hit_checksum = top_hit_checksum.wrapping_add(hit.chunk_id);
        }
    }

    let query_min = *query_durations.iter().min().unwrap_or(&Duration::ZERO);
    let query_max = *query_durations.iter().max().unwrap_or(&Duration::ZERO);
    let query_avg = average_duration(&query_durations);
    let p50 = percentile(query_durations.clone(), 50);
    let p95 = percentile(query_durations, 95);

    Ok(SyntheticBenchReport {
        footprint: estimate_footprint(&config),
        index_size,
        persisted_file_sizes,
        source_embedding_bytes: source_embedding_bytes(config.chunks, config.dimension),
        build_duration,
        vector_candidate_avg: non_empty_average_duration(&vector_candidate_durations),
        keyword_candidate_avg: non_empty_average_duration(&keyword_candidate_durations),
        query_min,
        query_avg,
        query_p50: p50,
        query_p95: p95,
        query_max,
        recall_at_k_vs_f32: recall_sum / config.queries as f64,
        total_hits,
        top_hit_checksum,
        config,
    })
}

fn benchmark_kernel(
    config: &KernelBenchConfig,
    dimension: usize,
    encoding: VectorEncoding,
) -> KernelBenchReport {
    match encoding {
        VectorEncoding::F32 => benchmark_f32_kernel(config, dimension),
        VectorEncoding::F16 => benchmark_f16_kernel(config, dimension),
        VectorEncoding::BF16 => benchmark_bf16_kernel(config, dimension),
        VectorEncoding::I8ScalarQuantized => benchmark_i8_kernel(config, dimension),
        VectorEncoding::BinaryQuantized => {
            panic!("BinaryQuantized is not supported by kernel benchmark")
        }
    }
}

#[derive(Debug, Clone)]
struct TopKBenchAccumulator {
    top_k: usize,
    bounded_vec_durations: Vec<Duration>,
    binary_heap_durations: Vec<Duration>,
    bounded_vec_checksum: u64,
    binary_heap_checksum: u64,
}

impl TopKBenchAccumulator {
    fn new(top_k: usize) -> Self {
        Self {
            top_k,
            bounded_vec_durations: Vec::new(),
            binary_heap_durations: Vec::new(),
            bounded_vec_checksum: 0,
            binary_heap_checksum: 0,
        }
    }

    fn record(&mut self, candidates: &[TopKCandidate]) {
        let start = Instant::now();
        let bounded_vec_hits = bounded_vec_top_k(candidates, self.top_k);
        self.bounded_vec_durations.push(start.elapsed());
        self.bounded_vec_checksum = self
            .bounded_vec_checksum
            .wrapping_add(topk_checksum(&bounded_vec_hits));

        let start = Instant::now();
        let binary_heap_hits = binary_heap_top_k(candidates, self.top_k);
        self.binary_heap_durations.push(start.elapsed());
        self.binary_heap_checksum = self
            .binary_heap_checksum
            .wrapping_add(topk_checksum(&binary_heap_hits));

        debug_assert_eq!(bounded_vec_hits, binary_heap_hits);
    }

    fn reports(self) -> [TopKBenchReport; 2] {
        [
            topk_report(
                self.top_k,
                TopKAlgorithm::BoundedVec,
                self.bounded_vec_durations,
                self.bounded_vec_checksum,
            ),
            topk_report(
                self.top_k,
                TopKAlgorithm::BinaryHeap,
                self.binary_heap_durations,
                self.binary_heap_checksum,
            ),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TopKCandidate {
    chunk_id: u64,
    score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HeapTopKCandidate(TopKCandidate);

impl Eq for HeapTopKCandidate {}

impl Ord for HeapTopKCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap. Make the worst ranked candidate compare
        // greatest so peek() returns the current replacement threshold.
        other
            .0
            .score
            .total_cmp(&self.0.score)
            .then_with(|| self.0.chunk_id.cmp(&other.0.chunk_id))
    }
}

impl PartialOrd for HeapTopKCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn topk_candidates(count: usize, seed: u64, query_id: usize) -> Vec<TopKCandidate> {
    let mut rng =
        DeterministicRng::new(seed ^ (query_id as u64).wrapping_mul(0x517c_c1b7_2722_0a95));
    (0..count)
        .map(|chunk_id| TopKCandidate {
            chunk_id: chunk_id as u64,
            score: rng.next_f32_signed(),
        })
        .collect()
}

fn bounded_vec_top_k(candidates: &[TopKCandidate], top_k: usize) -> Vec<TopKCandidate> {
    let mut hits = Vec::with_capacity(top_k);
    for candidate in candidates {
        if hits.len() < top_k {
            hits.push(*candidate);
            continue;
        }

        let Some(worst_index) = worst_topk_candidate_index(&hits) else {
            continue;
        };

        if topk_candidate_ranks_before(candidate, &hits[worst_index]) {
            hits[worst_index] = *candidate;
        }
    }
    sort_topk_candidates(&mut hits);
    hits
}

fn binary_heap_top_k(candidates: &[TopKCandidate], top_k: usize) -> Vec<TopKCandidate> {
    let mut heap = BinaryHeap::with_capacity(top_k);
    for candidate in candidates {
        if heap.len() < top_k {
            heap.push(HeapTopKCandidate(*candidate));
            continue;
        }

        let Some(worst) = heap.peek() else {
            continue;
        };

        if topk_candidate_ranks_before(candidate, &worst.0) {
            heap.pop();
            heap.push(HeapTopKCandidate(*candidate));
        }
    }

    let mut hits = heap
        .into_iter()
        .map(|candidate| candidate.0)
        .collect::<Vec<_>>();
    sort_topk_candidates(&mut hits);
    hits
}

fn worst_topk_candidate_index(hits: &[TopKCandidate]) -> Option<usize> {
    let mut worst_index = 0;
    for index in 1..hits.len() {
        if topk_candidate_ranks_before(&hits[worst_index], &hits[index]) {
            worst_index = index;
        }
    }
    Some(worst_index)
}

fn sort_topk_candidates(hits: &mut [TopKCandidate]) {
    hits.sort_by(compare_topk_candidates);
}

fn compare_topk_candidates(left: &TopKCandidate, right: &TopKCandidate) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn topk_candidate_ranks_before(left: &TopKCandidate, right: &TopKCandidate) -> bool {
    compare_topk_candidates(left, right).is_lt()
}

fn topk_checksum(hits: &[TopKCandidate]) -> u64 {
    hits.iter().fold(0u64, |checksum, hit| {
        checksum
            .wrapping_mul(16_777_619)
            .wrapping_add(hit.chunk_id)
            .wrapping_add(hit.score.to_bits() as u64)
    })
}

fn topk_report(
    top_k: usize,
    algorithm: TopKAlgorithm,
    durations: Vec<Duration>,
    checksum: u64,
) -> TopKBenchReport {
    let query_min = *durations.iter().min().unwrap_or(&Duration::ZERO);
    let query_max = *durations.iter().max().unwrap_or(&Duration::ZERO);
    let query_avg = average_duration(&durations);
    let query_p50 = percentile(durations.clone(), 50);
    let query_p95 = percentile(durations, 95);

    TopKBenchReport {
        top_k,
        algorithm,
        query_min,
        query_avg,
        query_p50,
        query_p95,
        query_max,
        checksum,
    }
}

fn topk_algorithm_name(algorithm: TopKAlgorithm) -> &'static str {
    match algorithm {
        TopKAlgorithm::BoundedVec => "bounded-vec",
        TopKAlgorithm::BinaryHeap => "binary-heap",
    }
}

fn benchmark_f32_kernel(config: &KernelBenchConfig, dimension: usize) -> KernelBenchReport {
    let values = flatten_generated_vectors(config.vectors, dimension, config.seed);
    let mut query_durations = Vec::with_capacity(config.queries);
    let mut score_checksum = 0.0;

    for query_id in 0..config.queries {
        let query = kernel_query(config, dimension, query_id);

        let start = Instant::now();
        for chunk in values.chunks_exact(dimension) {
            score_checksum += black_box(simd_dot_f32(&query, chunk));
        }
        query_durations.push(start.elapsed());
    }

    kernel_report(
        config.vectors,
        dimension,
        VectorEncoding::F32,
        values.len() * std::mem::size_of::<f32>(),
        query_durations,
        score_checksum,
    )
}

fn benchmark_f16_kernel(config: &KernelBenchConfig, dimension: usize) -> KernelBenchReport {
    let values = flatten_generated_vectors(config.vectors, dimension, config.seed)
        .into_iter()
        .map(f16::from_f32)
        .collect::<Vec<_>>();
    let mut query_durations = Vec::with_capacity(config.queries);
    let mut score_checksum = 0.0;

    for query_id in 0..config.queries {
        let query = kernel_query(config, dimension, query_id)
            .into_iter()
            .map(f16::from_f32)
            .collect::<Vec<_>>();

        let start = Instant::now();
        for chunk in values.chunks_exact(dimension) {
            score_checksum += black_box(simd_dot_f16(&query, chunk));
        }
        query_durations.push(start.elapsed());
    }

    kernel_report(
        config.vectors,
        dimension,
        VectorEncoding::F16,
        values.len() * std::mem::size_of::<f16>(),
        query_durations,
        score_checksum,
    )
}

fn benchmark_bf16_kernel(config: &KernelBenchConfig, dimension: usize) -> KernelBenchReport {
    let values = flatten_generated_vectors(config.vectors, dimension, config.seed)
        .into_iter()
        .map(bf16::from_f32)
        .collect::<Vec<_>>();
    let mut query_durations = Vec::with_capacity(config.queries);
    let mut score_checksum = 0.0;

    for query_id in 0..config.queries {
        let query = kernel_query(config, dimension, query_id)
            .into_iter()
            .map(bf16::from_f32)
            .collect::<Vec<_>>();

        let start = Instant::now();
        for chunk in values.chunks_exact(dimension) {
            score_checksum += black_box(simd_dot_bf16(&query, chunk));
        }
        query_durations.push(start.elapsed());
    }

    kernel_report(
        config.vectors,
        dimension,
        VectorEncoding::BF16,
        values.len() * std::mem::size_of::<bf16>(),
        query_durations,
        score_checksum,
    )
}

fn benchmark_i8_kernel(config: &KernelBenchConfig, dimension: usize) -> KernelBenchReport {
    let mut values = Vec::with_capacity(config.vectors * dimension);
    let mut scales = Vec::with_capacity(config.vectors);
    for vector_id in 0..config.vectors {
        let encoded = encode_i8_scalar_quantized(&generate_normalized_vector(
            dimension,
            config.seed,
            vector_id as u64,
        ));
        values.extend_from_slice(&encoded.values);
        scales.push(encoded.scale);
    }

    let mut query_durations = Vec::with_capacity(config.queries);
    let mut score_checksum = 0.0;

    for query_id in 0..config.queries {
        let query = encode_i8_scalar_quantized(&kernel_query(config, dimension, query_id));

        let start = Instant::now();
        for (row, chunk) in values.chunks_exact(dimension).enumerate() {
            score_checksum += black_box(
                simd_dot_i8(&query.values, chunk) * query.scale as f64 * scales[row] as f64,
            );
        }
        query_durations.push(start.elapsed());
    }

    kernel_report(
        config.vectors,
        dimension,
        VectorEncoding::I8ScalarQuantized,
        values.len() * std::mem::size_of::<i8>() + scales.len() * std::mem::size_of::<f32>(),
        query_durations,
        score_checksum,
    )
}

fn kernel_report(
    vectors: usize,
    dimension: usize,
    encoding: VectorEncoding,
    payload_bytes: usize,
    query_durations: Vec<Duration>,
    score_checksum: f64,
) -> KernelBenchReport {
    KernelBenchReport {
        vectors,
        dimension,
        encoding,
        payload_bytes,
        query_min: *query_durations.iter().min().unwrap_or(&Duration::ZERO),
        query_avg: average_duration(&query_durations),
        query_p50: percentile(query_durations.clone(), 50),
        query_p95: percentile(query_durations.clone(), 95),
        query_max: *query_durations.iter().max().unwrap_or(&Duration::ZERO),
        score_checksum,
    }
}

fn estimate_footprint(config: &SyntheticBenchConfig) -> FootprintEstimate {
    FootprintEstimate {
        vector_bytes: encoded_vector_bytes(config.chunks, config.dimension, config.encoding),
        chunk_fixed_bytes: config.chunks * chunk_fixed_bytes_per_chunk(),
        chunk_data_bytes: config.chunks * config.footprint.avg_chunk_data_bytes,
        metadata_bytes: config.chunks * config.footprint.avg_metadata_bytes,
        bm25_bytes: config.chunks
            * config.footprint.avg_bm25_terms_per_chunk
            * bm25_bytes_per_posting(),
        overhead_bytes: index_overhead_bytes(),
    }
}

fn build_synthetic_index(
    config: &SyntheticBenchConfig,
    encoding: VectorEncoding,
) -> Result<(ExactVectorIndex, Duration), CliError> {
    let index_config =
        IndexConfig::new(config.dimension, config.metric).with_vector_encoding(encoding);
    let mut index = ExactVectorIndex::try_with_config(index_config)?;

    let build_start = Instant::now();
    for chunk_id in 0..config.chunks {
        index.add_chunk(synthetic_chunk(
            config.dimension,
            config.seed,
            chunk_id,
            config.filter_every,
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
        text: synthetic_chunk_text(chunk_id),
        embedding: generate_normalized_vector(dimension, seed, chunk_id as u64),
        metadata,
        deleted: false,
        version: 1,
    }
}

fn synthetic_chunk_text(chunk_id: usize) -> String {
    format!(
        "synthetic chunk {chunk_id} topic{} group{} rareterm{}",
        chunk_id % 17,
        chunk_id % 101,
        chunk_id
    )
}

fn synthetic_query_text(target_chunk: usize) -> String {
    format!("rareterm{target_chunk} topic{}", target_chunk % 17)
}

fn synthetic_search_query(
    top_k: usize,
    embedding: Vec<f32>,
    filter_every: Option<usize>,
    target_chunk: usize,
) -> SearchQuery {
    let query = SearchQuery::new(embedding, top_k);

    match synthetic_filter(filter_every, target_chunk) {
        Some(filter) => query.with_filter(filter),
        None => query,
    }
}

fn synthetic_keyword_query(
    top_k: usize,
    text: &str,
    filter_every: Option<usize>,
    target_chunk: usize,
) -> KeywordQuery {
    let query = KeywordQuery::new(text, top_k);

    match synthetic_filter(filter_every, target_chunk) {
        Some(filter) => query.with_filter(filter),
        None => query,
    }
}

fn synthetic_hybrid_query(
    spec: SyntheticSearchSpec,
    embedding: Vec<f32>,
    text: &str,
    search_mode: SearchMode,
) -> HybridQuery {
    let query = HybridQuery::new(text, embedding, spec.top_k)
        .with_candidate_limits(spec.vector_candidates, spec.keyword_candidates);
    let query = match search_mode {
        SearchMode::HybridWeighted => query,
        SearchMode::HybridRrf => query.with_rrf_k(60.0),
        SearchMode::Vector | SearchMode::Keyword => query,
    };

    match synthetic_filter(spec.filter_every, spec.target_chunk) {
        Some(filter) => query.with_filter(filter),
        None => query,
    }
}

fn synthetic_filter(filter_every: Option<usize>, target_chunk: usize) -> Option<Filter> {
    filter_every.map(|filter_every| Filter::Equals {
        field: BENCH_FILTER_FIELD.to_owned(),
        value: MetadataValue::Integer((target_chunk % filter_every) as i64),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BenchHit {
    chunk_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SyntheticSearchSpec {
    top_k: usize,
    vector_candidates: usize,
    keyword_candidates: usize,
    filter_every: Option<usize>,
    target_chunk: usize,
}

fn run_synthetic_search_mode(
    index: &ExactVectorIndex,
    search_mode: SearchMode,
    spec: SyntheticSearchSpec,
    embedding: Vec<f32>,
    text: &str,
) -> Result<Vec<BenchHit>, CliError> {
    match search_mode {
        SearchMode::Vector => Ok(index
            .search(&synthetic_search_query(
                spec.top_k,
                embedding,
                spec.filter_every,
                spec.target_chunk,
            ))?
            .into_iter()
            .map(|hit| BenchHit {
                chunk_id: hit.chunk_id,
            })
            .collect()),
        SearchMode::Keyword => Ok(index
            .keyword_search(&synthetic_keyword_query(
                spec.top_k,
                text,
                spec.filter_every,
                spec.target_chunk,
            ))?
            .into_iter()
            .map(|hit| BenchHit {
                chunk_id: hit.chunk_id,
            })
            .collect()),
        SearchMode::HybridWeighted | SearchMode::HybridRrf => Ok(index
            .hybrid_search(&synthetic_hybrid_query(spec, embedding, text, search_mode))?
            .into_iter()
            .map(|hit| BenchHit {
                chunk_id: hit.chunk_id,
            })
            .collect()),
    }
}

fn recall_at_k(hits: &[BenchHit], ground_truth_hits: &[BenchHit]) -> f64 {
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

fn flatten_generated_vectors(vectors: usize, dimension: usize, seed: u64) -> Vec<f32> {
    let mut values = Vec::with_capacity(vectors * dimension);
    for vector_id in 0..vectors {
        values.extend_from_slice(&generate_normalized_vector(
            dimension,
            seed,
            vector_id as u64,
        ));
    }
    values
}

fn kernel_query(config: &KernelBenchConfig, dimension: usize, query_id: usize) -> Vec<f32> {
    generate_query_vector(
        dimension,
        config.seed,
        target_chunk_id(query_id, config.vectors) as u64,
        query_id,
    )
}

#[derive(Debug, Clone, PartialEq)]
struct I8EncodedVector {
    values: Vec<i8>,
    scale: f32,
}

fn encode_i8_scalar_quantized(embedding: &[f32]) -> I8EncodedVector {
    let max_abs = embedding
        .iter()
        .map(|value| value.abs())
        .fold(0.0, f32::max);

    if max_abs == 0.0 {
        return I8EncodedVector {
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

    I8EncodedVector { values, scale }
}

fn simd_dot_f32(left: &[f32], right: &[f32]) -> f64 {
    <f32 as SpatialSimilarity>::dot(left, right).unwrap_or(0.0)
}

fn simd_dot_f16(left: &[f16], right: &[f16]) -> f64 {
    <f16 as SpatialSimilarity>::dot(left, right).unwrap_or(0.0)
}

fn simd_dot_bf16(left: &[bf16], right: &[bf16]) -> f64 {
    <bf16 as SpatialSimilarity>::dot(left, right).unwrap_or(0.0)
}

fn simd_dot_i8(left: &[i8], right: &[i8]) -> f64 {
    diagnostic_dot_product_i8(left, right) as f64
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

fn non_empty_average_duration(durations: &[Duration]) -> Option<Duration> {
    (!durations.is_empty()).then(|| average_duration(durations))
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

fn mib_u64(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

fn signed_mib(bytes: isize) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

fn parse_positive(value: &str, name: &str) -> Result<usize, CliError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| CliError::InvalidArgument(format!("invalid numeric value for '{name}'")))?;

    if parsed == 0 {
        return Err(CliError::InvalidArgument(format!(
            "'{name}' must be greater than zero"
        )));
    }

    Ok(parsed)
}

fn parse_nonnegative(value: &str, name: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::InvalidArgument(format!("invalid numeric value for '{name}'")))
}

fn parse_mib(value: &str, name: &str) -> Result<usize, CliError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| CliError::InvalidArgument(format!("invalid MiB value for '{name}'")))?;

    if parsed <= 0.0 {
        return Err(CliError::InvalidArgument(format!(
            "'{name}' must be greater than zero"
        )));
    }

    Ok((parsed * 1024.0 * 1024.0).round() as usize)
}

fn parse_positive_list(value: &str, name: &str) -> Result<Vec<usize>, CliError> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| parse_positive(value, name))
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err(CliError::InvalidArgument(format!(
            "'{name}' must contain at least one value"
        )));
    }

    Ok(values)
}

fn parse_filter_every_list(value: &str, name: &str) -> Result<Vec<Option<usize>>, CliError> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "none" | "unfiltered" => Ok(None),
            _ => parse_positive(value, name).map(Some),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err(CliError::InvalidArgument(format!(
            "'{name}' must contain at least one value"
        )));
    }

    Ok(values)
}

fn parse_search_mode_list(value: &str) -> Result<Vec<SearchMode>, CliError> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_search_mode)
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err(CliError::InvalidArgument(
            "'--search-modes' must contain at least one value".to_owned(),
        ));
    }

    Ok(values)
}

fn parse_u64(value: &str, name: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::InvalidArgument(format!("invalid numeric value for '{name}'")))
}

fn parse_encoding_list(value: &str) -> Result<Vec<VectorEncoding>, CliError> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_encoding)
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err(CliError::InvalidArgument(
            "'--encodings' must contain at least one value".to_owned(),
        ));
    }

    Ok(values)
}

fn parse_search_mode(value: &str) -> Result<SearchMode, CliError> {
    match value.to_ascii_lowercase().as_str() {
        "vector" => Ok(SearchMode::Vector),
        "keyword" | "bm25" => Ok(SearchMode::Keyword),
        "hybrid-weighted" | "weighted" => Ok(SearchMode::HybridWeighted),
        "hybrid-rrf" | "rrf" => Ok(SearchMode::HybridRrf),
        _ => Err(CliError::InvalidArgument(format!(
            "unsupported search mode '{value}', expected vector, keyword, hybrid-weighted, or hybrid-rrf"
        ))),
    }
}

fn parse_encoding(value: &str) -> Result<VectorEncoding, CliError> {
    match value.to_ascii_lowercase().as_str() {
        "f32" => Ok(VectorEncoding::F32),
        "f16" => Ok(VectorEncoding::F16),
        "bf16" => Ok(VectorEncoding::BF16),
        "i8" | "i8-scalar" | "i8-scalar-quantized" => Ok(VectorEncoding::I8ScalarQuantized),
        _ => Err(CliError::InvalidArgument(format!(
            "unsupported encoding '{value}', expected f32, f16, bf16, or i8"
        ))),
    }
}

fn parse_metric(value: &str) -> Result<VectorMetric, CliError> {
    match value.to_ascii_lowercase().as_str() {
        "cosine" => Ok(VectorMetric::Cosine),
        "dot" | "dot-product" => Ok(VectorMetric::DotProduct),
        _ => Err(CliError::InvalidArgument(format!(
            "unsupported metric '{value}', expected cosine or dot"
        ))),
    }
}

fn search_mode_name(search_mode: SearchMode) -> &'static str {
    match search_mode {
        SearchMode::Vector => "vector",
        SearchMode::Keyword => "keyword",
        SearchMode::HybridWeighted => "hybrid-weighted",
        SearchMode::HybridRrf => "hybrid-rrf",
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

fn filter_every_name(filter_every: Option<usize>) -> String {
    filter_every
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

fn hybrid_candidate_limit_cell(search_mode: SearchMode, candidate_limit: usize) -> String {
    if is_hybrid_mode(search_mode) {
        candidate_limit.to_string()
    } else {
        "n/a".to_owned()
    }
}

fn persisted_mb_cell(file_sizes: Option<IndexFileSizeReport>) -> String {
    file_sizes
        .map(|file_sizes| format!("{:.3}", mib_u64(file_sizes.total_bytes())))
        .unwrap_or_else(|| "n/a".to_owned())
}

fn optional_millis_cell(duration: Option<Duration>) -> String {
    duration
        .map(|duration| format!("{:.3}", millis(duration)))
        .unwrap_or_else(|| "n/a".to_owned())
}

fn encoded_vector_bytes(chunks: usize, dimension: usize, encoding: VectorEncoding) -> usize {
    chunks * dimension * encoded_bytes_per_value(encoding)
        + chunks * encoded_sidecar_bytes_per_vector(encoding)
}

fn source_embedding_bytes(_chunks: usize, _dimension: usize) -> usize {
    0
}

fn chunk_fixed_bytes_per_chunk() -> usize {
    64
}

fn bm25_bytes_per_posting() -> usize {
    8
}

fn index_overhead_bytes() -> usize {
    4096
}

fn encoded_bytes_per_value(encoding: VectorEncoding) -> usize {
    match encoding {
        VectorEncoding::F32 => 4,
        VectorEncoding::F16 | VectorEncoding::BF16 => 2,
        VectorEncoding::I8ScalarQuantized => 1,
        VectorEncoding::BinaryQuantized => 0,
    }
}

fn encoded_sidecar_bytes_per_vector(encoding: VectorEncoding) -> usize {
    match encoding {
        VectorEncoding::I8ScalarQuantized => std::mem::size_of::<f32>(),
        VectorEncoding::F32
        | VectorEncoding::F16
        | VectorEncoding::BF16
        | VectorEncoding::BinaryQuantized => 0,
    }
}

#[derive(Debug)]
enum CliError {
    InvalidArgument(String),
    Core(vectorkit_core::VectorKitError),
}

impl CliError {
    fn usage() -> Self {
        Self::InvalidArgument(
            [
                "usage:",
                "  vectorkit bench synthetic [options]",
                "  vectorkit bench matrix [options]",
                "  vectorkit bench kernels [options]",
                "  vectorkit bench topk [options]",
                "  vectorkit bench memory [--config <scenario.json> | --config-json <json>]",
                "  vectorkit bench quality --fixture <fixture.json> [--qrels <qrels.tsv>] [--artifacts <directory>] [--iterations <n>]",
                "",
                "synthetic options:",
                "  --chunks <n>       default 1000",
                "  --dimension <n>    default 384",
                "  --queries <n>      default 100",
                "  --top-k <n>        default 10",
                "  --vector-candidates <n> hybrid vector candidate limit; default 50",
                "  --keyword-candidates <n> hybrid keyword candidate limit; default 50",
                "  --search-mode <kind> vector, keyword, hybrid-weighted, or hybrid-rrf; default vector",
                "  --encoding <kind>  f32, f16, bf16, or i8; default f32",
                "  --metric <kind>    cosine or dot; default cosine",
                "  --seed <n>         default 42",
                "  --filter-every <n> indexed equality filter with roughly 1/n selectivity",
                "  --persist-dir <path> save built index and report actual file sizes",
                "  --budget-mb <n>    footprint budget in MiB; default 20",
                "  --avg-chunk-data-bytes <n>  estimated bytes per chunk data; default 256",
                "  --avg-metadata-bytes <n>    estimated metadata bytes per chunk; default 32",
                "  --avg-bm25-terms <n>        estimated BM25 postings per chunk; default 24",
                "",
                "matrix options:",
                "  --chunks <n>          default 1000",
                "  --dimensions <list>   comma list; default 384,768,1536",
                "  --queries <n>         default 100",
                "  --top-k <list>        comma list; default 5,10",
                "  --vector-candidates <list> comma list of hybrid vector candidate limits; default 50",
                "  --keyword-candidates <list> comma list of hybrid keyword candidate limits; default 50",
                "  --search-modes <list> comma list of vector,keyword,hybrid-weighted,hybrid-rrf; default vector",
                "  --encodings <list>    comma list of f32,f16,bf16,i8; default f32,f16,bf16,i8",
                "  --metric <kind>       cosine or dot; default cosine",
                "  --seed <n>            default 42",
                "  --filter-every <n>    indexed equality filter with roughly 1/n selectivity",
                "  --filter-every-values <list> comma list of filter-every values or none; matrix only",
                "  --persist-dir <path>  save built indexes and report actual file sizes",
                "  --budget-mb <n>       footprint budget in MiB; default 20",
                "  --avg-chunk-data-bytes <n>  estimated bytes per chunk data; default 256",
                "  --avg-metadata-bytes <n>    estimated metadata bytes per chunk; default 32",
                "  --avg-bm25-terms <n>        estimated BM25 postings per chunk; default 24",
                "",
                "kernel options:",
                "  --vectors <n>        vectors scanned per query; default 24000",
                "  --dimensions <list>  comma list; default 384,768",
                "  --queries <n>        default 200",
                "  --encodings <list>   comma list of f32,f16,bf16,i8; default f32,f16,i8",
                "  --seed <n>           default 42",
                "",
                "topk options:",
                "  --candidates <n>     candidates per query; default 50000",
                "  --queries <n>        default 1000",
                "  --top-k <list>       comma list; default 5,10,50,100",
                "  --seed <n>           default 42",
            ]
            .join("\n"),
        )
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument(message) => write!(f, "{message}"),
            Self::Core(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CliError {}

impl From<vectorkit_core::VectorKitError> for CliError {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_synthetic_benchmark_config() {
        let config = SyntheticBenchConfig::parse(&[]).unwrap();

        assert_eq!(config, SyntheticBenchConfig::default());
    }

    #[test]
    fn parses_custom_synthetic_benchmark_config() {
        let args = [
            "--chunks",
            "10000",
            "--dimension",
            "768",
            "--queries",
            "50",
            "--top-k",
            "5",
            "--vector-candidates",
            "25",
            "--keyword-candidates",
            "30",
            "--search-mode",
            "hybrid-weighted",
            "--encoding",
            "i8",
            "--metric",
            "dot",
            "--seed",
            "7",
            "--filter-every",
            "10",
            "--persist-dir",
            "/tmp/vectorkit-synthetic",
            "--budget-mb",
            "12.5",
            "--avg-chunk-data-bytes",
            "128",
            "--avg-metadata-bytes",
            "16",
            "--avg-bm25-terms",
            "12",
        ]
        .map(str::to_owned);

        let config = SyntheticBenchConfig::parse(&args).unwrap();

        assert_eq!(config.chunks, 10_000);
        assert_eq!(config.dimension, 768);
        assert_eq!(config.queries, 50);
        assert_eq!(config.top_k, 5);
        assert_eq!(config.vector_candidates, 25);
        assert_eq!(config.keyword_candidates, 30);
        assert_eq!(config.search_mode, SearchMode::HybridWeighted);
        assert_eq!(config.encoding, VectorEncoding::I8ScalarQuantized);
        assert_eq!(config.metric, VectorMetric::DotProduct);
        assert_eq!(config.seed, 7);
        assert_eq!(config.filter_every, Some(10));
        assert_eq!(
            config.persist_dir,
            Some(PathBuf::from("/tmp/vectorkit-synthetic"))
        );
        assert_eq!(config.footprint.budget_bytes, 13_107_200);
        assert_eq!(config.footprint.avg_chunk_data_bytes, 128);
        assert_eq!(config.footprint.avg_metadata_bytes, 16);
        assert_eq!(config.footprint.avg_bm25_terms_per_chunk, 12);
    }

    #[test]
    fn generated_vectors_are_deterministic_and_normalized() {
        let first = generate_normalized_vector(8, 42, 1);
        let second = generate_normalized_vector(8, 42, 1);
        let norm = first.iter().map(|value| value * value).sum::<f32>().sqrt();

        assert_eq!(first, second);
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn parses_matrix_benchmark_config_lists() {
        let args = [
            "--chunks",
            "2000",
            "--dimensions",
            "384,768",
            "--top-k",
            "5,10",
            "--vector-candidates",
            "10,25",
            "--keyword-candidates",
            "10,50",
            "--search-modes",
            "vector,keyword,hybrid-rrf",
            "--encodings",
            "f32,i8",
            "--filter-every-values",
            "none,100,10,2",
            "--persist-dir",
            "/tmp/vectorkit-matrix",
        ]
        .map(str::to_owned);

        let config = MatrixBenchConfig::parse(&args).unwrap();

        assert_eq!(config.chunks, 2_000);
        assert_eq!(config.dimensions, vec![384, 768]);
        assert_eq!(config.top_ks, vec![5, 10]);
        assert_eq!(config.vector_candidate_limits, vec![10, 25]);
        assert_eq!(config.keyword_candidate_limits, vec![10, 50]);
        assert_eq!(
            config.search_modes,
            vec![
                SearchMode::Vector,
                SearchMode::Keyword,
                SearchMode::HybridRrf
            ]
        );
        assert_eq!(
            config.encodings,
            vec![VectorEncoding::F32, VectorEncoding::I8ScalarQuantized]
        );
        assert_eq!(
            config.filter_every_values,
            vec![None, Some(100), Some(10), Some(2)]
        );
        assert_eq!(
            config.persist_dir,
            Some(PathBuf::from("/tmp/vectorkit-matrix"))
        );
    }

    #[test]
    fn parses_matrix_legacy_filter_every_as_single_value() {
        let args = ["--filter-every", "100"].map(str::to_owned);

        let config = MatrixBenchConfig::parse(&args).unwrap();

        assert_eq!(config.filter_every_values, vec![Some(100)]);
    }

    #[test]
    fn candidate_limit_pairs_only_expand_hybrid_modes() {
        assert_eq!(
            candidate_limit_pairs(SearchMode::Keyword, &[10, 25], &[10, 50]),
            vec![(10, 10)]
        );
        assert_eq!(
            candidate_limit_pairs(SearchMode::HybridWeighted, &[10, 25], &[10, 50]),
            vec![(10, 10), (10, 50), (25, 10), (25, 50)]
        );
    }

    #[test]
    fn parses_kernel_benchmark_config() {
        let args = [
            "--vectors",
            "50000",
            "--dimensions",
            "384,768",
            "--queries",
            "25",
            "--encodings",
            "f32,i8",
            "--seed",
            "9",
        ]
        .map(str::to_owned);

        let config = KernelBenchConfig::parse(&args).unwrap();

        assert_eq!(config.vectors, 50_000);
        assert_eq!(config.dimensions, vec![384, 768]);
        assert_eq!(config.queries, 25);
        assert_eq!(
            config.encodings,
            vec![VectorEncoding::F32, VectorEncoding::I8ScalarQuantized]
        );
        assert_eq!(config.seed, 9);
    }

    #[test]
    fn estimates_vector_memory_by_encoding() {
        assert_eq!(encoded_vector_bytes(10, 8, VectorEncoding::F32), 320);
        assert_eq!(encoded_vector_bytes(10, 8, VectorEncoding::F16), 160);
        assert_eq!(
            encoded_vector_bytes(10, 8, VectorEncoding::I8ScalarQuantized),
            120
        );
        assert_eq!(source_embedding_bytes(10, 8), 0);
    }

    #[test]
    fn estimates_total_footprint_components() {
        let config = SyntheticBenchConfig {
            chunks: 10,
            dimension: 8,
            encoding: VectorEncoding::I8ScalarQuantized,
            footprint: FootprintConfig {
                budget_bytes: 1024,
                avg_chunk_data_bytes: 100,
                avg_metadata_bytes: 10,
                avg_bm25_terms_per_chunk: 2,
            },
            ..SyntheticBenchConfig::default()
        };

        let estimate = estimate_footprint(&config);

        assert_eq!(estimate.vector_bytes, 120);
        assert_eq!(estimate.chunk_fixed_bytes, 640);
        assert_eq!(estimate.chunk_data_bytes, 1_000);
        assert_eq!(estimate.metadata_bytes, 100);
        assert_eq!(estimate.bm25_bytes, 160);
        assert_eq!(estimate.overhead_bytes, 4_096);
        assert_eq!(estimate.total_bytes(), 6_116);
        assert_eq!(
            estimate.budget_headroom_bytes(config.footprint.budget_bytes),
            -5_092
        );
    }

    #[test]
    fn i8_scalar_quantization_uses_per_vector_scale() {
        let encoded = encode_i8_scalar_quantized(&[0.0, 0.5, -1.0]);

        assert_eq!(encoded.scale, 1.0 / 127.0);
        assert_eq!(encoded.values, vec![0, 64, -127]);
    }

    #[test]
    fn recall_at_k_counts_overlap_with_f32_ground_truth() {
        let hits = vec![bench_hit(1), bench_hit(2), bench_hit(3), bench_hit(4)];
        let ground_truth_hits = vec![bench_hit(2), bench_hit(4), bench_hit(6), bench_hit(8)];

        assert_eq!(recall_at_k(&hits, &ground_truth_hits), 0.5);
    }

    #[test]
    fn recall_at_k_is_complete_when_ground_truth_is_empty() {
        assert_eq!(recall_at_k(&[bench_hit(1)], &[]), 1.0);
    }

    #[test]
    fn parses_search_modes() {
        assert_eq!(parse_search_mode("vector").unwrap(), SearchMode::Vector);
        assert_eq!(parse_search_mode("bm25").unwrap(), SearchMode::Keyword);
        assert_eq!(
            parse_search_mode("hybrid-weighted").unwrap(),
            SearchMode::HybridWeighted
        );
        assert_eq!(parse_search_mode("rrf").unwrap(), SearchMode::HybridRrf);
    }

    #[test]
    fn synthetic_keyword_query_matches_target_chunk_text() {
        let text = synthetic_chunk_text(42);
        let query_text = synthetic_query_text(42);

        assert!(text.contains("rareterm42"));
        assert!(query_text.contains("rareterm42"));
    }

    fn bench_hit(chunk_id: u64) -> BenchHit {
        BenchHit { chunk_id }
    }
}
