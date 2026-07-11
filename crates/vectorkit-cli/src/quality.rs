use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vectorkit_core::{
    Chunk, ChunkInput, Document, ExactVectorIndex, Filter, HybridQuery, IndexConfig, Metadata,
    MetadataValue, SearchQuery, VectorEncoding, VectorMetric,
};

pub(crate) struct QualityOutcome {
    pub(crate) json: String,
    pub(crate) passed: bool,
}

pub(crate) fn run(args: &[String]) -> Result<QualityOutcome, String> {
    let config = Config::parse(args)?;
    let raw = fs::read_to_string(&config.fixture_path).map_err(|error| {
        format!(
            "failed to read quality fixture '{}': {error}",
            config.fixture_path.display()
        )
    })?;
    let fixture: Fixture =
        serde_json::from_str(&raw).map_err(|error| format!("invalid quality fixture: {error}"))?;
    fixture.validate()?;
    let report = benchmark(&fixture, config.iterations)?;
    let passed = report.gates.passed;
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("failed to serialize quality report: {error}"))?;
    Ok(QualityOutcome { json, passed })
}

#[derive(Debug)]
struct Config {
    fixture_path: PathBuf,
    iterations: usize,
}

impl Config {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut fixture_path = None;
        let mut iterations = 20usize;
        let mut offset = 0;
        while offset < args.len() {
            let flag = &args[offset];
            let value = args
                .get(offset + 1)
                .ok_or_else(|| format!("missing value for '{flag}'"))?;
            match flag.as_str() {
                "--fixture" => fixture_path = Some(PathBuf::from(value)),
                "--iterations" => {
                    iterations = value
                        .parse()
                        .map_err(|_| "--iterations must be a positive integer".to_owned())?;
                    if iterations == 0 {
                        return Err("--iterations must be greater than zero".to_owned());
                    }
                }
                _ => return Err(format!("unknown quality benchmark argument '{flag}'")),
            }
            offset += 2;
        }
        Ok(Self {
            fixture_path: fixture_path.ok_or_else(|| {
                "usage: vectorkit bench quality --fixture <fixture.json> [--iterations <n>]"
                    .to_owned()
            })?,
            iterations,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    schema_version: u32,
    fixture_id: String,
    model: ModelInfo,
    top_k: usize,
    candidate_pairs: Vec<[usize; 2]>,
    default_pair: [usize; 2],
    quality_gates: QualityGates,
    documents: Vec<FixtureDocument>,
    deletions: Vec<String>,
    replacements: Vec<FixtureReplacement>,
    queries: Vec<FixtureQuery>,
    embedding_provenance: EmbeddingProvenance,
}

impl Fixture {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported quality fixture schema version {}",
                self.schema_version
            ));
        }
        if self.top_k == 0 || self.queries.is_empty() || self.documents.is_empty() {
            return Err(
                "quality fixture requires documents, queries, and positive top_k".to_owned(),
            );
        }
        if !self.candidate_pairs.contains(&self.default_pair) {
            return Err("default_pair must appear in candidate_pairs".to_owned());
        }
        if self
            .candidate_pairs
            .iter()
            .any(|pair| pair[0] < self.top_k || pair[1] < self.top_k)
        {
            return Err("candidate limits must be at least top_k".to_owned());
        }
        let dimension = self.model.dimension;
        for document in &self.documents {
            validate_embedding(&document.id, &document.embedding, dimension)?;
        }
        for replacement in &self.replacements {
            validate_embedding(
                &replacement.document_id,
                &replacement.initial_embedding,
                dimension,
            )?;
            validate_embedding(
                &replacement.document_id,
                &replacement.replacement_embedding,
                dimension,
            )?;
        }
        for query in &self.queries {
            validate_embedding(&query.id, &query.embedding, dimension)?;
            if query.relevance.is_empty() {
                return Err(format!("query '{}' has no relevance judgments", query.id));
            }
        }
        Ok(())
    }
}

fn validate_embedding(id: &str, embedding: &[f32], dimension: usize) -> Result<(), String> {
    if embedding.len() != dimension {
        return Err(format!(
            "'{id}' embedding has dimension {}, expected {dimension}",
            embedding.len()
        ));
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err(format!("'{id}' embedding contains a non-finite value"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct ModelInfo {
    id: String,
    slug: String,
    sequence_length: usize,
    dimension: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct EmbeddingProvenance {
    generator: String,
    model: String,
    sequence_length: usize,
    normalized: bool,
}

#[derive(Debug, Deserialize)]
struct FixtureDocument {
    id: String,
    text: String,
    metadata: BTreeMap<String, String>,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct FixtureReplacement {
    document_id: String,
    initial_text: String,
    replacement_text: String,
    metadata: BTreeMap<String, String>,
    initial_embedding: Vec<f32>,
    replacement_embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct FixtureQuery {
    id: String,
    category: String,
    text: String,
    embedding: Vec<f32>,
    relevance: BTreeMap<String, u8>,
    #[serde(default)]
    filter: Option<FixtureFilter>,
    #[serde(default)]
    forbidden_document_ids: Vec<String>,
    #[serde(default)]
    required_text: Option<RequiredText>,
}

#[derive(Debug, Deserialize)]
struct FixtureFilter {
    field: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct RequiredText {
    document_id: String,
    contains: String,
}

#[derive(Debug, Deserialize)]
struct QualityGates {
    min_ndcg_at_k: f64,
    min_mrr: f64,
    min_recall_vs_reference: f64,
    min_i8_recall_vs_f32: f64,
    min_i8_vector_recall_vs_f32: f64,
}

#[derive(Debug, Serialize)]
struct QualityReport<'a> {
    schema_version: u32,
    fixture_id: &'a str,
    model: &'a ModelInfo,
    embedding_provenance: &'a EmbeddingProvenance,
    documents: usize,
    queries: usize,
    top_k: usize,
    iterations: usize,
    reference_pair: [usize; 2],
    default_pair: [usize; 2],
    indexes: Vec<IndexProfile>,
    runs: Vec<QualityRun>,
    vector_only_runs: Vec<VectorOnlyRun>,
    categories: Vec<CategorySummary>,
    gates: GateResult,
}

#[derive(Debug, Serialize)]
struct QualityRun {
    encoding: &'static str,
    vector_candidates: usize,
    keyword_candidates: usize,
    relevance_recall_at_k: f64,
    mrr: f64,
    ndcg_at_k: f64,
    recall_at_k_vs_same_encoding_reference: f64,
    recall_at_k_vs_f32_reference: f64,
    latency: LatencyStats,
    lifecycle_violations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VectorOnlyRun {
    encoding: &'static str,
    top_k: usize,
    relevance_recall_at_k: f64,
    mrr: f64,
    ndcg_at_k: f64,
    recall_at_k_vs_f32: f64,
    top_1_agreement_vs_f32: f64,
    ordered_result_agreement_vs_f32: f64,
    differences_vs_f32: Vec<VectorResultDifference>,
    latency: LatencyStats,
    lifecycle_violations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VectorResultDifference {
    query_id: String,
    f32_document_ids: Vec<String>,
    encoding_document_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct IndexProfile {
    encoding: &'static str,
    estimated_in_memory_payload_bytes: usize,
    persisted_bytes: u64,
    load_ms: f64,
    post_load_rankings_match: bool,
    post_load_vector_rankings_match: bool,
}

#[derive(Debug, Serialize)]
struct CategorySummary {
    category: String,
    queries: usize,
    default_i8_ndcg_at_k: f64,
    default_i8_mrr: f64,
}

#[derive(Debug, Serialize)]
struct LatencyStats {
    samples: usize,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Serialize)]
struct GateResult {
    passed: bool,
    violations: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct RankedDocument {
    document_id: String,
    chunk_id: u64,
}

fn benchmark(fixture: &Fixture, iterations: usize) -> Result<QualityReport<'_>, String> {
    let reference_pair = fixture
        .candidate_pairs
        .iter()
        .copied()
        .max_by_key(|pair| pair[0].saturating_add(pair[1]))
        .ok_or_else(|| "candidate_pairs cannot be empty".to_owned())?;
    let directory = TemporaryDirectory::new()?;
    let f32_built = build_index(fixture, VectorEncoding::F32)?;
    let f32_before_load = reference_results(&f32_built, fixture, reference_pair)?;
    let f32_vector_before_load = vector_results(&f32_built, fixture, 10)?;
    let (f32, f32_profile) = persist_reload(
        f32_built,
        directory.path.join("f32"),
        "f32",
        fixture,
        reference_pair,
        &f32_before_load,
        &f32_vector_before_load,
    )?;
    let i8_built = build_index(fixture, VectorEncoding::I8ScalarQuantized)?;
    let i8_before_load = reference_results(&i8_built, fixture, reference_pair)?;
    let i8_vector_before_load = vector_results(&i8_built, fixture, 10)?;
    let (i8, i8_profile) = persist_reload(
        i8_built,
        directory.path.join("i8"),
        "i8",
        fixture,
        reference_pair,
        &i8_before_load,
        &i8_vector_before_load,
    )?;
    let f32_reference = reference_results(&f32, fixture, reference_pair)?;
    let i8_reference = reference_results(&i8, fixture, reference_pair)?;
    let mut runs = Vec::new();

    for (encoding, index, same_reference) in
        [("f32", &f32, &f32_reference), ("i8", &i8, &i8_reference)]
    {
        for &pair in &fixture.candidate_pairs {
            runs.push(benchmark_pair(
                fixture,
                index,
                encoding,
                pair,
                same_reference,
                &f32_reference,
                iterations,
            )?);
        }
    }

    let vector_only_runs = benchmark_vector_only(fixture, &f32, &i8, iterations)?;

    let categories = category_summaries(fixture, &i8, fixture.default_pair)?;
    let gates = evaluate_gates(fixture, &runs, &vector_only_runs);
    Ok(QualityReport {
        schema_version: 1,
        fixture_id: &fixture.fixture_id,
        model: &fixture.model,
        embedding_provenance: &fixture.embedding_provenance,
        documents: fixture.documents.len() + fixture.replacements.len(),
        queries: fixture.queries.len(),
        top_k: fixture.top_k,
        iterations,
        reference_pair,
        default_pair: fixture.default_pair,
        indexes: vec![f32_profile, i8_profile],
        runs,
        vector_only_runs,
        categories,
        gates,
    })
}

fn persist_reload(
    index: ExactVectorIndex,
    path: PathBuf,
    encoding: &'static str,
    fixture: &Fixture,
    reference_pair: [usize; 2],
    before_load: &[Vec<RankedDocument>],
    vector_before_load: &[Vec<RankedDocument>],
) -> Result<(ExactVectorIndex, IndexProfile), String> {
    let estimated_in_memory_payload_bytes = index.size_estimate().total_bytes();
    let files = index
        .save_to_dir(&path)
        .map_err(|error| error.to_string())?;
    drop(index);
    let started = Instant::now();
    let loaded = ExactVectorIndex::load_from_dir(&path).map_err(|error| error.to_string())?;
    let load_ms = millis(started.elapsed());
    let after_load = reference_results(&loaded, fixture, reference_pair)?;
    let vector_after_load = vector_results(&loaded, fixture, 10)?;
    let profile = IndexProfile {
        encoding,
        estimated_in_memory_payload_bytes,
        persisted_bytes: files.total_bytes(),
        load_ms,
        post_load_rankings_match: before_load == after_load,
        post_load_vector_rankings_match: vector_before_load == vector_after_load,
    };
    Ok((loaded, profile))
}

fn build_index(fixture: &Fixture, encoding: VectorEncoding) -> Result<ExactVectorIndex, String> {
    let mut index = ExactVectorIndex::try_with_config(
        IndexConfig::new(fixture.model.dimension, VectorMetric::Cosine)
            .with_vector_encoding(encoding),
    )
    .map_err(|error| error.to_string())?;
    let mut chunk_id = 0u64;
    for document in &fixture.documents {
        index
            .add_chunk(Chunk {
                chunk_id,
                document_id: document.id.clone(),
                text: document.text.clone(),
                embedding: document.embedding.clone(),
                metadata: metadata(&document.metadata),
                deleted: false,
                version: 1,
            })
            .map_err(|error| error.to_string())?;
        chunk_id += 1;
    }
    for replacement in &fixture.replacements {
        index
            .add_chunk(Chunk {
                chunk_id,
                document_id: replacement.document_id.clone(),
                text: replacement.initial_text.clone(),
                embedding: replacement.initial_embedding.clone(),
                metadata: metadata(&replacement.metadata),
                deleted: false,
                version: 1,
            })
            .map_err(|error| error.to_string())?;
        chunk_id += 1;
        index
            .upsert_document(
                Document {
                    id: replacement.document_id.clone(),
                    text: replacement.replacement_text.clone(),
                    metadata: metadata(&replacement.metadata),
                },
                vec![ChunkInput {
                    text: replacement.replacement_text.clone(),
                    embedding: replacement.replacement_embedding.clone(),
                    metadata: Metadata::new(),
                }],
            )
            .map_err(|error| error.to_string())?;
    }
    for document_id in &fixture.deletions {
        if index.delete_document(document_id) == 0 {
            return Err(format!(
                "fixture deletion references inactive or missing document '{document_id}'"
            ));
        }
    }
    Ok(index)
}

fn metadata(values: &BTreeMap<String, String>) -> Metadata {
    values
        .iter()
        .map(|(key, value)| (key.clone(), MetadataValue::String(value.clone())))
        .collect()
}

fn reference_results(
    index: &ExactVectorIndex,
    fixture: &Fixture,
    pair: [usize; 2],
) -> Result<Vec<Vec<RankedDocument>>, String> {
    fixture
        .queries
        .iter()
        .map(|query| search(index, query, fixture.top_k, pair))
        .collect()
}

fn vector_results(
    index: &ExactVectorIndex,
    fixture: &Fixture,
    top_k: usize,
) -> Result<Vec<Vec<RankedDocument>>, String> {
    fixture
        .queries
        .iter()
        .map(|query| vector_search(index, query, top_k))
        .collect()
}

fn benchmark_vector_only(
    fixture: &Fixture,
    f32: &ExactVectorIndex,
    i8: &ExactVectorIndex,
    iterations: usize,
) -> Result<Vec<VectorOnlyRun>, String> {
    let depths = BTreeSet::from([fixture.top_k, 10]);
    let mut runs = Vec::with_capacity(depths.len() * 2);
    for top_k in depths {
        let f32_reference = vector_results(f32, fixture, top_k)?;
        runs.push(benchmark_vector_encoding(
            fixture,
            f32,
            "f32",
            top_k,
            &f32_reference,
            iterations,
        )?);
        runs.push(benchmark_vector_encoding(
            fixture,
            i8,
            "i8",
            top_k,
            &f32_reference,
            iterations,
        )?);
    }
    Ok(runs)
}

fn benchmark_vector_encoding(
    fixture: &Fixture,
    index: &ExactVectorIndex,
    encoding: &'static str,
    top_k: usize,
    f32_reference: &[Vec<RankedDocument>],
    iterations: usize,
) -> Result<VectorOnlyRun, String> {
    let results = vector_results(index, fixture, top_k)?;
    let relevance_recall_at_k = average(
        fixture
            .queries
            .iter()
            .zip(&results)
            .map(|(query, hits)| relevance_recall(query, hits)),
    );
    let mrr = average(
        fixture
            .queries
            .iter()
            .zip(&results)
            .map(|(query, hits)| reciprocal_rank(query, hits)),
    );
    let ndcg_at_k = average(
        fixture
            .queries
            .iter()
            .zip(&results)
            .map(|(query, hits)| ndcg(query, hits, top_k)),
    );
    let recall_at_k_vs_f32 = average(
        results
            .iter()
            .zip(f32_reference)
            .map(|(hits, reference)| overlap_recall(hits, reference)),
    );
    let top_1_agreement_vs_f32 = average(
        results
            .iter()
            .zip(f32_reference)
            .map(|(hits, reference)| top_1_agreement(hits, reference)),
    );
    let ordered_result_agreement_vs_f32 = average(
        results
            .iter()
            .zip(f32_reference)
            .map(|(hits, reference)| f64::from(hits == reference)),
    );
    let differences_vs_f32 = fixture
        .queries
        .iter()
        .zip(&results)
        .zip(f32_reference)
        .filter(|((_, hits), reference)| hits != reference)
        .map(|((query, hits), reference)| VectorResultDifference {
            query_id: query.id.clone(),
            f32_document_ids: reference
                .iter()
                .map(|hit| hit.document_id.clone())
                .collect(),
            encoding_document_ids: hits.iter().map(|hit| hit.document_id.clone()).collect(),
        })
        .collect();
    let lifecycle_violations = lifecycle_violations(fixture, index, &results);

    for query in &fixture.queries {
        black_box(vector_search(index, query, top_k)?);
    }
    let mut durations = Vec::with_capacity(iterations * fixture.queries.len());
    for _ in 0..iterations {
        for query in &fixture.queries {
            let start = Instant::now();
            black_box(vector_search(index, query, top_k)?);
            durations.push(start.elapsed());
        }
    }
    Ok(VectorOnlyRun {
        encoding,
        top_k,
        relevance_recall_at_k,
        mrr,
        ndcg_at_k,
        recall_at_k_vs_f32,
        top_1_agreement_vs_f32,
        ordered_result_agreement_vs_f32,
        differences_vs_f32,
        latency: latency_stats(durations),
        lifecycle_violations,
    })
}

fn benchmark_pair(
    fixture: &Fixture,
    index: &ExactVectorIndex,
    encoding: &'static str,
    pair: [usize; 2],
    same_reference: &[Vec<RankedDocument>],
    f32_reference: &[Vec<RankedDocument>],
    iterations: usize,
) -> Result<QualityRun, String> {
    let results = fixture
        .queries
        .iter()
        .map(|query| search(index, query, fixture.top_k, pair))
        .collect::<Result<Vec<_>, _>>()?;
    let relevance_recall_at_k = average(
        fixture
            .queries
            .iter()
            .zip(&results)
            .map(|(query, hits)| relevance_recall(query, hits)),
    );
    let mrr = average(
        fixture
            .queries
            .iter()
            .zip(&results)
            .map(|(query, hits)| reciprocal_rank(query, hits)),
    );
    let ndcg_at_k = average(
        fixture
            .queries
            .iter()
            .zip(&results)
            .map(|(query, hits)| ndcg(query, hits, fixture.top_k)),
    );
    let recall_at_k_vs_same_encoding_reference = average(
        results
            .iter()
            .zip(same_reference)
            .map(|(hits, reference)| overlap_recall(hits, reference)),
    );
    let recall_at_k_vs_f32_reference = average(
        results
            .iter()
            .zip(f32_reference)
            .map(|(hits, reference)| overlap_recall(hits, reference)),
    );
    let lifecycle_violations = lifecycle_violations(fixture, index, &results);

    for query in &fixture.queries {
        black_box(search(index, query, fixture.top_k, pair)?);
    }
    let mut durations = Vec::with_capacity(iterations * fixture.queries.len());
    for _ in 0..iterations {
        for query in &fixture.queries {
            let start = Instant::now();
            black_box(search(index, query, fixture.top_k, pair)?);
            durations.push(start.elapsed());
        }
    }
    Ok(QualityRun {
        encoding,
        vector_candidates: pair[0],
        keyword_candidates: pair[1],
        relevance_recall_at_k,
        mrr,
        ndcg_at_k,
        recall_at_k_vs_same_encoding_reference,
        recall_at_k_vs_f32_reference,
        latency: latency_stats(durations),
        lifecycle_violations,
    })
}

fn search(
    index: &ExactVectorIndex,
    query: &FixtureQuery,
    top_k: usize,
    pair: [usize; 2],
) -> Result<Vec<RankedDocument>, String> {
    let mut request = HybridQuery::new(&query.text, query.embedding.clone(), top_k)
        .with_rrf_k(60.0)
        .with_candidate_limits(pair[0], pair[1]);
    if let Some(filter) = &query.filter {
        request = request.with_filter(Filter::Equals {
            field: filter.field.clone(),
            value: MetadataValue::String(filter.value.clone()),
        });
    }
    index
        .hybrid_search(&request)
        .map_err(|error| error.to_string())
        .map(|hits| {
            hits.into_iter()
                .map(|hit| RankedDocument {
                    document_id: hit.document_id,
                    chunk_id: hit.chunk_id,
                })
                .collect()
        })
}

fn vector_search(
    index: &ExactVectorIndex,
    query: &FixtureQuery,
    top_k: usize,
) -> Result<Vec<RankedDocument>, String> {
    let mut request = SearchQuery::new(query.embedding.clone(), top_k);
    if let Some(filter) = &query.filter {
        request = request.with_filter(Filter::Equals {
            field: filter.field.clone(),
            value: MetadataValue::String(filter.value.clone()),
        });
    }
    index
        .search(&request)
        .map_err(|error| error.to_string())
        .map(|hits| {
            hits.into_iter()
                .map(|hit| RankedDocument {
                    document_id: hit.document_id,
                    chunk_id: hit.chunk_id,
                })
                .collect()
        })
}

fn relevance_recall(query: &FixtureQuery, hits: &[RankedDocument]) -> f64 {
    let relevant = query
        .relevance
        .iter()
        .filter(|(_, grade)| **grade > 0)
        .map(|(document_id, _)| document_id)
        .collect::<BTreeSet<_>>();
    if relevant.is_empty() {
        return 1.0;
    }
    let found = hits
        .iter()
        .filter(|hit| relevant.contains(&hit.document_id))
        .count();
    found as f64 / relevant.len() as f64
}

fn reciprocal_rank(query: &FixtureQuery, hits: &[RankedDocument]) -> f64 {
    hits.iter()
        .position(|hit| query.relevance.get(&hit.document_id).copied().unwrap_or(0) > 0)
        .map(|offset| 1.0 / (offset + 1) as f64)
        .unwrap_or(0.0)
}

fn ndcg(query: &FixtureQuery, hits: &[RankedDocument], top_k: usize) -> f64 {
    let dcg = hits
        .iter()
        .take(top_k)
        .enumerate()
        .map(|(offset, hit)| {
            let grade = query.relevance.get(&hit.document_id).copied().unwrap_or(0);
            gain(grade, offset)
        })
        .sum::<f64>();
    let mut grades = query.relevance.values().copied().collect::<Vec<_>>();
    grades.sort_unstable_by(|left, right| right.cmp(left));
    let ideal = grades
        .into_iter()
        .take(top_k)
        .enumerate()
        .map(|(offset, grade)| gain(grade, offset))
        .sum::<f64>();
    if ideal == 0.0 {
        1.0
    } else {
        dcg / ideal
    }
}

fn gain(grade: u8, offset: usize) -> f64 {
    (2f64.powi(i32::from(grade)) - 1.0) / (offset as f64 + 2.0).log2()
}

fn overlap_recall(hits: &[RankedDocument], reference: &[RankedDocument]) -> f64 {
    if reference.is_empty() {
        return 1.0;
    }
    let documents = hits
        .iter()
        .map(|hit| &hit.document_id)
        .collect::<BTreeSet<_>>();
    reference
        .iter()
        .filter(|hit| documents.contains(&hit.document_id))
        .count() as f64
        / reference.len() as f64
}

fn top_1_agreement(hits: &[RankedDocument], reference: &[RankedDocument]) -> f64 {
    match (hits.first(), reference.first()) {
        (Some(hit), Some(expected)) => f64::from(hit.document_id == expected.document_id),
        (None, None) => 1.0,
        _ => 0.0,
    }
}

fn lifecycle_violations(
    fixture: &Fixture,
    index: &ExactVectorIndex,
    results: &[Vec<RankedDocument>],
) -> Vec<String> {
    let mut violations = Vec::new();
    for (query, hits) in fixture.queries.iter().zip(results) {
        for forbidden in &query.forbidden_document_ids {
            if hits.iter().any(|hit| &hit.document_id == forbidden) {
                violations.push(format!(
                    "query '{}' returned forbidden document '{}'",
                    query.id, forbidden
                ));
            }
        }
        if let Some(required) = &query.required_text {
            match hits
                .iter()
                .find(|hit| hit.document_id == required.document_id)
                .and_then(|hit| index.chunk(hit.chunk_id))
            {
                Some(chunk) if chunk.text.contains(&required.contains) => {}
                Some(_) => violations.push(format!(
                    "query '{}' returned stale replacement text for '{}'",
                    query.id, required.document_id
                )),
                None => violations.push(format!(
                    "query '{}' did not return required replacement '{}'",
                    query.id, required.document_id
                )),
            }
        }
    }
    violations
}

fn category_summaries(
    fixture: &Fixture,
    index: &ExactVectorIndex,
    pair: [usize; 2],
) -> Result<Vec<CategorySummary>, String> {
    let mut values: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for query in &fixture.queries {
        let hits = search(index, query, fixture.top_k, pair)?;
        values.entry(query.category.clone()).or_default().push((
            ndcg(query, &hits, fixture.top_k),
            reciprocal_rank(query, &hits),
        ));
    }
    Ok(values
        .into_iter()
        .map(|(category, scores)| CategorySummary {
            category,
            queries: scores.len(),
            default_i8_ndcg_at_k: scores.iter().map(|score| score.0).sum::<f64>()
                / scores.len() as f64,
            default_i8_mrr: scores.iter().map(|score| score.1).sum::<f64>() / scores.len() as f64,
        })
        .collect())
}

fn evaluate_gates(
    fixture: &Fixture,
    runs: &[QualityRun],
    vector_only_runs: &[VectorOnlyRun],
) -> GateResult {
    let mut violations = Vec::new();
    for run in runs
        .iter()
        .filter(|run| [run.vector_candidates, run.keyword_candidates] == fixture.default_pair)
    {
        if run.ndcg_at_k < fixture.quality_gates.min_ndcg_at_k {
            violations.push(format!(
                "{} default NDCG@{} {:.4} is below {:.4}",
                run.encoding, fixture.top_k, run.ndcg_at_k, fixture.quality_gates.min_ndcg_at_k
            ));
        }
        if run.mrr < fixture.quality_gates.min_mrr {
            violations.push(format!(
                "{} default MRR {:.4} is below {:.4}",
                run.encoding, run.mrr, fixture.quality_gates.min_mrr
            ));
        }
        if run.recall_at_k_vs_same_encoding_reference
            < fixture.quality_gates.min_recall_vs_reference
        {
            violations.push(format!(
                "{} default recall vs same-encoding reference {:.4} is below {:.4}",
                run.encoding,
                run.recall_at_k_vs_same_encoding_reference,
                fixture.quality_gates.min_recall_vs_reference
            ));
        }
        if run.encoding == "i8"
            && run.recall_at_k_vs_f32_reference < fixture.quality_gates.min_i8_recall_vs_f32
        {
            violations.push(format!(
                "i8 default recall vs F32 reference {:.4} is below {:.4}",
                run.recall_at_k_vs_f32_reference, fixture.quality_gates.min_i8_recall_vs_f32
            ));
        }
        violations.extend(run.lifecycle_violations.iter().cloned());
    }
    for run in vector_only_runs.iter().filter(|run| run.encoding == "i8") {
        if run.recall_at_k_vs_f32 < fixture.quality_gates.min_i8_vector_recall_vs_f32 {
            violations.push(format!(
                "i8 vector-only recall@{} vs F32 {:.4} is below {:.4}",
                run.top_k,
                run.recall_at_k_vs_f32,
                fixture.quality_gates.min_i8_vector_recall_vs_f32
            ));
        }
        violations.extend(run.lifecycle_violations.iter().cloned());
    }
    GateResult {
        passed: violations.is_empty(),
        violations,
    }
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    values.iter().sum::<f64>() / values.len() as f64
}

fn latency_stats(mut durations: Vec<Duration>) -> LatencyStats {
    durations.sort_unstable();
    let mean = durations.iter().map(Duration::as_secs_f64).sum::<f64>() / durations.len() as f64;
    LatencyStats {
        samples: durations.len(),
        mean_ms: mean * 1_000.0,
        p50_ms: millis(percentile(&durations, 50)),
        p95_ms: millis(percentile(&durations, 95)),
        p99_ms: millis(percentile(&durations, 99)),
        max_ms: millis(*durations.last().unwrap_or(&Duration::ZERO)),
    }
}

fn percentile(values: &[Duration], percentile: usize) -> Duration {
    let rank = (values.len() * percentile).div_ceil(100);
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new() -> Result<Self, String> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vectorkit-quality-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(Self { path })
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
    fn ndcg_rewards_relevant_documents_in_judged_order() {
        let query = FixtureQuery {
            id: "q".to_owned(),
            category: "test".to_owned(),
            text: "query".to_owned(),
            embedding: vec![1.0],
            relevance: BTreeMap::from([("best".to_owned(), 3), ("good".to_owned(), 1)]),
            filter: None,
            forbidden_document_ids: Vec::new(),
            required_text: None,
        };
        let ideal = vec![ranked("best", 0), ranked("good", 1)];
        let reversed = vec![ranked("good", 1), ranked("best", 0)];
        assert!((ndcg(&query, &ideal, 2) - 1.0).abs() < 1e-9);
        assert!(ndcg(&query, &reversed, 2) < 1.0);
    }

    #[test]
    fn overlap_recall_uses_reference_result_count() {
        let hits = vec![ranked("a", 0), ranked("b", 1)];
        let reference = vec![ranked("a", 0), ranked("c", 2)];
        assert_eq!(overlap_recall(&hits, &reference), 0.5);
    }

    #[test]
    fn top_1_agreement_compares_the_first_document() {
        assert_eq!(top_1_agreement(&[ranked("a", 0)], &[ranked("a", 1)]), 1.0);
        assert_eq!(top_1_agreement(&[ranked("a", 0)], &[ranked("b", 1)]), 0.0);
        assert_eq!(top_1_agreement(&[], &[]), 1.0);
    }

    #[test]
    fn checked_in_fixture_passes_quality_gates() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/retrieval-quality/v1/fixture.json");
        let outcome = run(&[
            "--fixture".to_owned(),
            fixture.display().to_string(),
            "--iterations".to_owned(),
            "1".to_owned(),
        ])
        .unwrap();
        assert!(outcome.passed, "{}", outcome.json);
    }

    fn ranked(document_id: &str, chunk_id: u64) -> RankedDocument {
        RankedDocument {
            document_id: document_id.to_owned(),
            chunk_id,
        }
    }
}
