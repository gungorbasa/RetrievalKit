use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

use vectorkit_core::{
    Chunk, ExactVectorIndex, Filter, IndexConfig, IndexSizeEstimate, Metadata, MetadataValue,
    SearchHit, SearchQuery, VectorEncoding, VectorMetric,
};

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
        _ => Err(CliError::usage()),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SyntheticBenchConfig {
    chunks: usize,
    dimension: usize,
    queries: usize,
    top_k: usize,
    encoding: VectorEncoding,
    metric: VectorMetric,
    seed: u64,
    filter_every: Option<usize>,
    footprint: FootprintConfig,
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
            encoding: VectorEncoding::F32,
            metric: VectorMetric::Cosine,
            seed: 42,
            filter_every: None,
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
                "--encoding" => config.encoding = parse_encoding(value)?,
                "--metric" => config.metric = parse_metric(value)?,
                "--seed" => config.seed = parse_u64(value, flag)?,
                "--filter-every" => config.filter_every = Some(parse_positive(value, flag)?),
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
    encodings: Vec<VectorEncoding>,
    metric: VectorMetric,
    seed: u64,
    filter_every: Option<usize>,
    footprint: FootprintConfig,
}

impl Default for MatrixBenchConfig {
    fn default() -> Self {
        Self {
            chunks: 1_000,
            dimensions: vec![384, 768, 1536],
            queries: 100,
            top_ks: vec![5, 10],
            encodings: vec![
                VectorEncoding::F32,
                VectorEncoding::F16,
                VectorEncoding::BF16,
                VectorEncoding::I8ScalarQuantized,
            ],
            metric: VectorMetric::Cosine,
            seed: 42,
            filter_every: None,
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
                "--encodings" => config.encodings = parse_encoding_list(value)?,
                "--metric" => config.metric = parse_metric(value)?,
                "--seed" => config.seed = parse_u64(value, flag)?,
                "--filter-every" => config.filter_every = Some(parse_positive(value, flag)?),
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

#[derive(Debug, Clone)]
struct SyntheticBenchReport {
    config: SyntheticBenchConfig,
    footprint: FootprintEstimate,
    index_size: IndexSizeEstimate,
    source_embedding_bytes: usize,
    build_duration: Duration,
    query_min: Duration,
    query_avg: Duration,
    query_p50: Duration,
    query_p95: Duration,
    query_max: Duration,
    recall_at_k_vs_f32: f64,
    total_hits: usize,
    top_hit_checksum: u64,
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
    println!("build_ms: {:.3}", millis(report.build_duration));
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
        "| chunks | dim | top_k | enc | metric | filter every | vector MB | aux MB | est total MB | current payload MB | headroom MB | retained f32 MB | build ms | min ms | avg ms | p50 ms | p95 ms | max ms | recall@k vs f32 | hits | checksum |"
    );
    println!(
        "|---:|---:|---:|:---|:---|:---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    );

    for dimension in &config.dimensions {
        for top_k in &config.top_ks {
            for encoding in &config.encodings {
                let report = benchmark_synthetic(SyntheticBenchConfig {
                    chunks: config.chunks,
                    dimension: *dimension,
                    queries: config.queries,
                    top_k: *top_k,
                    encoding: *encoding,
                    metric: config.metric,
                    seed: config.seed,
                    filter_every: config.filter_every,
                    footprint: config.footprint.clone(),
                })?;

                println!(
                    "| {} | {} | {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.4} | {} | {} |",
                    report.config.chunks,
                    report.config.dimension,
                    report.config.top_k,
                    encoding_name(report.config.encoding),
                    metric_name(report.config.metric),
                    filter_every_name(report.config.filter_every),
                    mib(report.footprint.vector_bytes),
                    mib(report.footprint.auxiliary_bytes()),
                    mib(report.footprint.total_bytes()),
                    mib(report.index_size.total_bytes()),
                    signed_mib(
                        report
                            .footprint
                            .budget_headroom_bytes(report.config.footprint.budget_bytes)
                    ),
                    mib(report.source_embedding_bytes),
                    millis(report.build_duration),
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

    Ok(())
}

fn benchmark_synthetic(config: SyntheticBenchConfig) -> Result<SyntheticBenchReport, CliError> {
    let (index, build_duration) = build_synthetic_index(&config, config.encoding)?;
    let index_size = index.size_estimate();
    let f32_ground_truth = if config.encoding == VectorEncoding::F32 {
        None
    } else {
        Some(build_synthetic_index(&config, VectorEncoding::F32)?.0)
    };

    let mut query_durations = Vec::with_capacity(config.queries);
    let mut recall_sum = 0.0;
    let mut total_hits = 0usize;
    let mut top_hit_checksum = 0u64;

    for query_id in 0..config.queries {
        let target_chunk = target_chunk_id(query_id, config.chunks);
        let query =
            generate_query_vector(config.dimension, config.seed, target_chunk as u64, query_id);
        let search_query = synthetic_search_query(
            config.top_k,
            query.clone(),
            config.filter_every,
            target_chunk,
        );

        let start = Instant::now();
        let hits = index.search(&search_query)?;
        query_durations.push(start.elapsed());

        recall_sum += match &f32_ground_truth {
            Some(ground_truth) => {
                let ground_truth_query =
                    synthetic_search_query(config.top_k, query, config.filter_every, target_chunk);
                let ground_truth_hits = ground_truth.search(&ground_truth_query)?;
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
        source_embedding_bytes: source_embedding_bytes(config.chunks, config.dimension),
        build_duration,
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

fn mib(bytes: usize) -> f64 {
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
                "",
                "synthetic options:",
                "  --chunks <n>       default 1000",
                "  --dimension <n>    default 384",
                "  --queries <n>      default 100",
                "  --top-k <n>        default 10",
                "  --encoding <kind>  f32, f16, bf16, or i8; default f32",
                "  --metric <kind>    cosine or dot; default cosine",
                "  --seed <n>         default 42",
                "  --filter-every <n> indexed equality filter with roughly 1/n selectivity",
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
                "  --encodings <list>    comma list of f32,f16,bf16,i8; default f32,f16,bf16,i8",
                "  --metric <kind>       cosine or dot; default cosine",
                "  --seed <n>            default 42",
                "  --filter-every <n>    indexed equality filter with roughly 1/n selectivity",
                "  --budget-mb <n>       footprint budget in MiB; default 20",
                "  --avg-chunk-data-bytes <n>  estimated bytes per chunk data; default 256",
                "  --avg-metadata-bytes <n>    estimated metadata bytes per chunk; default 32",
                "  --avg-bm25-terms <n>        estimated BM25 postings per chunk; default 24",
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
            "--encoding",
            "i8",
            "--metric",
            "dot",
            "--seed",
            "7",
            "--filter-every",
            "10",
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
        assert_eq!(config.encoding, VectorEncoding::I8ScalarQuantized);
        assert_eq!(config.metric, VectorMetric::DotProduct);
        assert_eq!(config.seed, 7);
        assert_eq!(config.filter_every, Some(10));
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
            "--encodings",
            "f32,i8",
            "--filter-every",
            "100",
        ]
        .map(str::to_owned);

        let config = MatrixBenchConfig::parse(&args).unwrap();

        assert_eq!(config.chunks, 2_000);
        assert_eq!(config.dimensions, vec![384, 768]);
        assert_eq!(config.top_ks, vec![5, 10]);
        assert_eq!(
            config.encodings,
            vec![VectorEncoding::F32, VectorEncoding::I8ScalarQuantized]
        );
        assert_eq!(config.filter_every, Some(100));
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
    fn recall_at_k_counts_overlap_with_f32_ground_truth() {
        let hits = vec![search_hit(1), search_hit(2), search_hit(3), search_hit(4)];
        let ground_truth_hits = vec![search_hit(2), search_hit(4), search_hit(6), search_hit(8)];

        assert_eq!(recall_at_k(&hits, &ground_truth_hits), 0.5);
    }

    #[test]
    fn recall_at_k_is_complete_when_ground_truth_is_empty() {
        assert_eq!(recall_at_k(&[search_hit(1)], &[]), 1.0);
    }

    fn search_hit(chunk_id: u64) -> SearchHit {
        SearchHit {
            chunk_id,
            document_id: format!("doc-{chunk_id}"),
            score: 1.0,
            trace: vectorkit_core::SearchTrace {
                vector_score: 1.0,
                keyword_score: None,
                filter_matched: true,
            },
        }
    }
}
