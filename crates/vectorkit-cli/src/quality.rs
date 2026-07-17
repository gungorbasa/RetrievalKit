use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vectorkit_core::{
    Chunk, ChunkInput, Document, ExactVectorIndex, Filter, HybridQuery, IndexConfig, KeywordQuery,
    Metadata, MetadataValue, SearchQuery, VectorEncoding, VectorMetric,
};

mod artifacts;
pub(crate) mod v3;
mod v3_canonical;
mod v3_execution;
mod v3_execution_status;
mod v3_graph_execution;
mod v3_graph_input;
mod v3_graph_retrieval_execution;
pub(crate) mod v3_hotpotqa;
mod v3_ingestion;
mod v3_population;
mod v3_runs;
mod v3_schema;
mod v3_seed;
mod v3_validation;

pub(crate) struct QualityOutcome {
    pub(crate) json: String,
    pub(crate) passed: bool,
}

pub(crate) fn run(args: &[String]) -> Result<QualityOutcome, String> {
    let config = Config::parse(args)?;
    let fixture = load_fixture(&config.fixture_path, config.qrels_path.as_deref())?;
    fixture.validate()?;
    let output = benchmark(&fixture, config.iterations, config.artifacts_path.is_some())?;
    if let Some(path) = &config.artifacts_path {
        artifacts::write(path, &fixture, &output.artifact_runs)?;
    }
    let passed = output.report.gates.passed;
    let json = serde_json::to_string_pretty(&output.report)
        .map_err(|error| format!("failed to serialize quality report: {error}"))?;
    Ok(QualityOutcome { json, passed })
}

#[derive(Debug)]
struct Config {
    fixture_path: PathBuf,
    iterations: usize,
    artifacts_path: Option<PathBuf>,
    qrels_path: Option<PathBuf>,
}

impl Config {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut fixture_path = None;
        let mut iterations = 20usize;
        let mut artifacts_path = None;
        let mut qrels_path = None;
        let mut offset = 0;
        while offset < args.len() {
            let flag = &args[offset];
            let value = args
                .get(offset + 1)
                .ok_or_else(|| format!("missing value for '{flag}'"))?;
            match flag.as_str() {
                "--fixture" => fixture_path = Some(PathBuf::from(value)),
                "--artifacts" => artifacts_path = Some(PathBuf::from(value)),
                "--qrels" => qrels_path = Some(PathBuf::from(value)),
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
                "usage: vectorkit bench quality --fixture <fixture.json> [--qrels <qrels.tsv>] [--artifacts <directory>] [--iterations <n>]".to_owned()
            })?,
            iterations,
            artifacts_path,
            qrels_path,
        })
    }
}

#[derive(Debug)]
struct Fixture {
    schema_version: u32,
    fixture_id: String,
    model: ModelInfo,
    top_k: usize,
    evaluation_depth: usize,
    candidate_pairs: Vec<[usize; 2]>,
    default_pair: [usize; 2],
    quality_gates: QualityGates,
    documents: Vec<FixtureDocument>,
    deletions: Vec<String>,
    replacements: Vec<FixtureReplacement>,
    queries: Vec<FixtureQuery>,
    embedding_provenance: EmbeddingProvenance,
    dataset_provenance: Option<DatasetProvenance>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyFixture {
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionV2 {
    schema_version: u32,
    collection_id: String,
    model: ModelInfo,
    top_k: usize,
    evaluation_depth: usize,
    candidate_pairs: Vec<[usize; 2]>,
    default_pair: [usize; 2],
    #[serde(default)]
    quality_gates: QualityGates,
    documents_path: PathBuf,
    queries_path: PathBuf,
    #[serde(default)]
    qrels_path: Option<PathBuf>,
    embedding_provenance: EmbeddingProvenance,
    #[serde(default)]
    dataset_provenance: Option<DatasetProvenance>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DatasetProvenance {
    name: String,
    split: String,
    source_url: String,
    checksum: String,
    preprocessing: String,
    corpus_documents: usize,
    queries: usize,
    qrels: usize,
}

fn load_fixture(path: &Path, qrels_override: Option<&Path>) -> Result<Fixture, String> {
    let raw = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read quality fixture '{}': {error}",
            path.display()
        )
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("invalid quality fixture: {error}"))?;
    let schema_version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "quality fixture requires integer schema_version".to_owned())?;
    let mut fixture = match schema_version {
        1 => {
            let legacy: LegacyFixture = serde_json::from_value(value)
                .map_err(|error| format!("invalid quality fixture: {error}"))?;
            Fixture {
                schema_version: legacy.schema_version,
                fixture_id: legacy.fixture_id,
                model: legacy.model,
                top_k: legacy.top_k,
                evaluation_depth: legacy.top_k.max(10),
                candidate_pairs: legacy.candidate_pairs,
                default_pair: legacy.default_pair,
                quality_gates: legacy.quality_gates,
                documents: legacy.documents,
                deletions: legacy.deletions,
                replacements: legacy.replacements,
                queries: legacy.queries,
                embedding_provenance: legacy.embedding_provenance,
                dataset_provenance: None,
            }
        }
        2 => load_collection_v2(path, value, qrels_override)?,
        version => {
            return Err(format!(
                "unsupported quality fixture schema version {version}"
            ))
        }
    };
    if schema_version == 1 {
        if let Some(qrels_path) = qrels_override {
            apply_qrels(&mut fixture, qrels_path)?;
        }
    }
    Ok(fixture)
}

fn load_collection_v2(
    manifest_path: &Path,
    value: serde_json::Value,
    qrels_override: Option<&Path>,
) -> Result<Fixture, String> {
    let collection: CollectionV2 = serde_json::from_value(value)
        .map_err(|error| format!("invalid quality collection: {error}"))?;
    let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let documents = load_json_lines::<FixtureDocument>(&base.join(&collection.documents_path))?;
    let queries = load_json_lines::<FixtureQuery>(&base.join(&collection.queries_path))?;
    let mut fixture = Fixture {
        schema_version: collection.schema_version,
        fixture_id: collection.collection_id,
        model: collection.model,
        top_k: collection.top_k,
        evaluation_depth: collection.evaluation_depth,
        candidate_pairs: collection.candidate_pairs,
        default_pair: collection.default_pair,
        quality_gates: collection.quality_gates,
        documents,
        deletions: Vec::new(),
        replacements: Vec::new(),
        queries,
        embedding_provenance: collection.embedding_provenance,
        dataset_provenance: collection.dataset_provenance,
    };
    let qrels_path = qrels_override
        .map(Path::to_path_buf)
        .or_else(|| collection.qrels_path.map(|path| base.join(path)))
        .ok_or_else(|| {
            "schema version 2 quality collection requires qrels_path or --qrels".to_owned()
        })?;
    apply_qrels(&mut fixture, &qrels_path)?;
    Ok(fixture)
}

fn load_json_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(offset, line)| {
            serde_json::from_str(line).map_err(|error| {
                format!(
                    "invalid JSON at '{}':{}: {error}",
                    path.display(),
                    offset + 1
                )
            })
        })
        .collect()
}

fn apply_qrels(fixture: &mut Fixture, path: &Path) -> Result<(), String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read qrels '{}': {error}", path.display()))?;
    let mut judgments = BTreeMap::<String, BTreeMap<String, u8>>::new();
    for (offset, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err(format!(
                "invalid qrels '{}':{}: expected four whitespace-separated fields",
                path.display(),
                offset + 1
            ));
        }
        validate_trec_id(fields[0], "query ID")?;
        validate_trec_id(fields[2], "document ID")?;
        let grade = fields[3].parse::<u8>().map_err(|_| {
            format!(
                "invalid qrels '{}':{}: relevance must be an integer from 0 through 127",
                path.display(),
                offset + 1
            )
        })?;
        if grade > 127 {
            return Err(format!(
                "invalid qrels '{}':{}: relevance must be at most 127",
                path.display(),
                offset + 1
            ));
        }
        if judgments
            .entry(fields[0].to_owned())
            .or_default()
            .insert(fields[2].to_owned(), grade)
            .is_some()
        {
            return Err(format!(
                "invalid qrels '{}':{}: duplicate judgment for query '{}' and document '{}'",
                path.display(),
                offset + 1,
                fields[0],
                fields[2]
            ));
        }
    }
    let query_ids = fixture
        .queries
        .iter()
        .map(|query| query.id.as_str())
        .collect::<BTreeSet<_>>();
    let unknown_queries = judgments
        .keys()
        .filter(|query_id| !query_ids.contains(query_id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown_queries.is_empty() {
        return Err(format!(
            "qrels reference unknown query IDs: {}",
            unknown_queries.join(", ")
        ));
    }
    for query in &mut fixture.queries {
        query.relevance = judgments.remove(&query.id).unwrap_or_default();
    }
    Ok(())
}

fn validate_trec_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().any(char::is_whitespace) || value.contains('\0') {
        Err(format!("{label} '{value}' is not a valid TREC token"))
    } else {
        Ok(())
    }
}

impl Fixture {
    fn validate(&self) -> Result<(), String> {
        if !matches!(self.schema_version, 1 | 2) {
            return Err(format!(
                "unsupported quality fixture schema version {}",
                self.schema_version
            ));
        }
        if self.top_k == 0
            || self.evaluation_depth < self.top_k
            || self.queries.is_empty()
            || self.documents.is_empty()
        {
            return Err(
                "quality fixture requires documents, queries, positive top_k, and evaluation_depth >= top_k"
                    .to_owned(),
            );
        }
        if !self.candidate_pairs.contains(&self.default_pair) {
            return Err("default_pair must appear in candidate_pairs".to_owned());
        }
        if self.candidate_pairs.iter().collect::<BTreeSet<_>>().len() != self.candidate_pairs.len()
        {
            return Err("candidate_pairs must not contain duplicates".to_owned());
        }
        if self
            .candidate_pairs
            .iter()
            .any(|pair| pair[0] < self.top_k || pair[1] < self.top_k)
        {
            return Err("candidate limits must be at least top_k".to_owned());
        }
        let dimension = self.model.dimension;
        let mut document_ids = BTreeSet::new();
        for document in &self.documents {
            validate_trec_id(&document.id, "document ID")?;
            if !document_ids.insert(document.id.as_str()) {
                return Err(format!("duplicate document ID '{}'", document.id));
            }
            validate_embedding(&document.id, &document.embedding, dimension)?;
        }
        for replacement in &self.replacements {
            validate_trec_id(&replacement.document_id, "document ID")?;
            if !document_ids.insert(replacement.document_id.as_str()) {
                return Err(format!(
                    "duplicate document ID '{}'",
                    replacement.document_id
                ));
            }
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
        let mut query_ids = BTreeSet::new();
        for query in &self.queries {
            validate_trec_id(&query.id, "query ID")?;
            if !query_ids.insert(query.id.as_str()) {
                return Err(format!("duplicate query ID '{}'", query.id));
            }
            validate_embedding(&query.id, &query.embedding, dimension)?;
            if !query.relevance.values().any(|grade| *grade > 0) {
                return Err(format!(
                    "query '{}' has no positive relevance judgments",
                    query.id
                ));
            }
            for document_id in query.relevance.keys() {
                if !document_ids.contains(document_id.as_str())
                    && !self
                        .replacements
                        .iter()
                        .any(|replacement| replacement.document_id == *document_id)
                {
                    return Err(format!(
                        "query '{}' judges unknown document '{}'",
                        query.id, document_id
                    ));
                }
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
    #[serde(default)]
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

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct QualityGates {
    #[serde(default)]
    min_relevance_recall_at_k: Option<f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    dataset_provenance: Option<&'a DatasetProvenance>,
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
    build_ms: f64,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
struct RankedDocument {
    document_id: String,
    chunk_id: u64,
    score: f32,
}

struct BenchmarkOutput<'a> {
    report: QualityReport<'a>,
    artifact_runs: Vec<artifacts::ArtifactRun>,
}

fn benchmark(
    fixture: &Fixture,
    iterations: usize,
    include_artifacts: bool,
) -> Result<BenchmarkOutput<'_>, String> {
    let reference_pair = fixture
        .candidate_pairs
        .iter()
        .copied()
        .max_by_key(|pair| pair[0].saturating_add(pair[1]))
        .ok_or_else(|| "candidate_pairs cannot be empty".to_owned())?;
    let directory = TemporaryDirectory::new()?;
    let started = Instant::now();
    let f32_built = build_index(fixture, VectorEncoding::F32)?;
    let f32_build_ms = millis(started.elapsed());
    let f32_before_load = reference_results(&f32_built, fixture, reference_pair)?;
    let f32_vector_before_load = vector_results(&f32_built, fixture, 10)?;
    let (f32, f32_profile) = persist_reload(
        f32_built,
        directory.path.join("f32"),
        "f32",
        fixture,
        reference_pair,
        PersistenceReference {
            hybrid: &f32_before_load,
            vector: &f32_vector_before_load,
        },
        f32_build_ms,
    )?;
    let started = Instant::now();
    let i8_built = build_index(fixture, VectorEncoding::I8ScalarQuantized)?;
    let i8_build_ms = millis(started.elapsed());
    let i8_before_load = reference_results(&i8_built, fixture, reference_pair)?;
    let i8_vector_before_load = vector_results(&i8_built, fixture, 10)?;
    let (i8, i8_profile) = persist_reload(
        i8_built,
        directory.path.join("i8"),
        "i8",
        fixture,
        reference_pair,
        PersistenceReference {
            hybrid: &i8_before_load,
            vector: &i8_vector_before_load,
        },
        i8_build_ms,
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
    let artifact_runs = if include_artifacts {
        build_artifact_runs(fixture, &f32, &i8)?
    } else {
        Vec::new()
    };
    Ok(BenchmarkOutput {
        report: QualityReport {
            schema_version: 1,
            fixture_id: &fixture.fixture_id,
            model: &fixture.model,
            embedding_provenance: &fixture.embedding_provenance,
            dataset_provenance: fixture.dataset_provenance.as_ref(),
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
        },
        artifact_runs,
    })
}

struct PersistenceReference<'a> {
    hybrid: &'a [Vec<RankedDocument>],
    vector: &'a [Vec<RankedDocument>],
}

fn persist_reload(
    index: ExactVectorIndex,
    path: PathBuf,
    encoding: &'static str,
    fixture: &Fixture,
    reference_pair: [usize; 2],
    reference: PersistenceReference<'_>,
    build_ms: f64,
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
    let post_load_rankings_match = reference.hybrid == after_load;
    let post_load_vector_rankings_match = reference.vector == vector_after_load;
    if !post_load_rankings_match || !post_load_vector_rankings_match {
        return Err(format!(
            "{encoding} rankings changed after persistence reload"
        ));
    }
    let profile = IndexProfile {
        encoding,
        estimated_in_memory_payload_bytes,
        persisted_bytes: files.total_bytes(),
        build_ms,
        load_ms,
        post_load_rankings_match,
        post_load_vector_rankings_match,
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
    search_raw(index, query, top_k, pair).map(|hits| deduplicate_documents(hits).0)
}

fn search_raw(
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
                    score: hit.score,
                })
                .collect()
        })
}

fn vector_search(
    index: &ExactVectorIndex,
    query: &FixtureQuery,
    top_k: usize,
) -> Result<Vec<RankedDocument>, String> {
    vector_search_raw(index, query, top_k).map(|hits| deduplicate_documents(hits).0)
}

fn vector_search_raw(
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
                    score: hit.score,
                })
                .collect()
        })
}

fn keyword_search_raw(
    index: &ExactVectorIndex,
    query: &FixtureQuery,
    top_k: usize,
) -> Result<Vec<RankedDocument>, String> {
    let mut request = KeywordQuery::new(&query.text, top_k);
    if let Some(filter) = &query.filter {
        request = request.with_filter(Filter::Equals {
            field: filter.field.clone(),
            value: MetadataValue::String(filter.value.clone()),
        });
    }
    index
        .keyword_search(&request)
        .map_err(|error| error.to_string())
        .map(|hits| {
            hits.into_iter()
                .map(|hit| RankedDocument {
                    document_id: hit.document_id,
                    chunk_id: hit.chunk_id,
                    score: hit.score,
                })
                .collect()
        })
}

fn weighted_search_raw(
    index: &ExactVectorIndex,
    query: &FixtureQuery,
    top_k: usize,
    pair: [usize; 2],
    alpha: f32,
) -> Result<Vec<RankedDocument>, String> {
    let mut request = HybridQuery::new(&query.text, query.embedding.clone(), top_k)
        .with_alpha(alpha)
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
                    score: hit.score,
                })
                .collect()
        })
}

fn deduplicate_documents(hits: Vec<RankedDocument>) -> (Vec<RankedDocument>, usize) {
    let original_len = hits.len();
    let mut seen = BTreeSet::new();
    let hits = hits
        .into_iter()
        .filter(|hit| seen.insert(hit.document_id.clone()))
        .collect::<Vec<_>>();
    let duplicates = original_len - hits.len();
    (hits, duplicates)
}

fn build_artifact_runs(
    fixture: &Fixture,
    f32: &ExactVectorIndex,
    i8: &ExactVectorIndex,
) -> Result<Vec<artifacts::ArtifactRun>, String> {
    let depth = fixture.evaluation_depth;
    let mut runs = Vec::new();
    let exact_reference_documents = exact_reference_documents(fixture);
    runs.push(artifacts::ArtifactRun::new(
        format!("exact-reference-f32-k{depth}"),
        "exact_reference",
        "f32",
        depth,
        collect_artifact_hits(fixture, |query| {
            Ok(exact_reference_search(
                &exact_reference_documents,
                query,
                depth,
            ))
        })?,
    ));
    for (encoding, index) in [("f32", f32), ("i8", i8)] {
        runs.push(artifacts::ArtifactRun::new(
            format!("vector-{encoding}-k{depth}"),
            "vector",
            encoding,
            depth,
            collect_artifact_hits(fixture, |query| vector_search_raw(index, query, depth))?,
        ));
        for &pair in &fixture.candidate_pairs {
            runs.push(
                artifacts::ArtifactRun::new(
                    format!("hybrid-rrf-{encoding}-v{}-b{}-k{depth}", pair[0], pair[1]),
                    "hybrid_rrf",
                    encoding,
                    depth,
                    collect_artifact_hits(fixture, |query| search_raw(index, query, depth, pair))?,
                )
                .with_candidate_limits(pair[0], pair[1]),
            );
        }
        for alpha_percent in [25u8, 50, 75] {
            let alpha = f32::from(alpha_percent) / 100.0;
            runs.push(
                artifacts::ArtifactRun::new(
                    format!(
                        "hybrid-weighted-a{alpha_percent:03}-{encoding}-v{}-b{}-k{depth}",
                        fixture.default_pair[0], fixture.default_pair[1]
                    ),
                    "hybrid_weighted",
                    encoding,
                    depth,
                    collect_artifact_hits(fixture, |query| {
                        weighted_search_raw(index, query, depth, fixture.default_pair, alpha)
                    })?,
                )
                .with_candidate_limits(fixture.default_pair[0], fixture.default_pair[1])
                .with_alpha(alpha),
            );
        }
    }
    runs.push(
        artifacts::ArtifactRun::new(
            format!("bm25-k{depth}"),
            "bm25",
            "none",
            depth,
            collect_artifact_hits(fixture, |query| keyword_search_raw(f32, query, depth))?,
        )
        .with_keyword_candidates(depth),
    );
    runs.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    Ok(runs)
}

struct ExactReferenceDocument<'a> {
    document_id: &'a str,
    chunk_id: u64,
    metadata: &'a BTreeMap<String, String>,
    normalized_embedding: Vec<f32>,
}

fn exact_reference_documents(fixture: &Fixture) -> Vec<ExactReferenceDocument<'_>> {
    let deleted = fixture
        .deletions
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut documents = fixture
        .documents
        .iter()
        .enumerate()
        .filter(|(_, document)| !deleted.contains(document.id.as_str()))
        .map(|(offset, document)| ExactReferenceDocument {
            document_id: &document.id,
            chunk_id: offset as u64,
            metadata: &document.metadata,
            normalized_embedding: normalized(&document.embedding),
        })
        .collect::<Vec<_>>();
    documents.extend(
        fixture
            .replacements
            .iter()
            .enumerate()
            .filter(|(_, replacement)| !deleted.contains(replacement.document_id.as_str()))
            .map(|(offset, replacement)| ExactReferenceDocument {
                document_id: &replacement.document_id,
                chunk_id: (fixture.documents.len() + (offset * 2) + 1) as u64,
                metadata: &replacement.metadata,
                normalized_embedding: normalized(&replacement.replacement_embedding),
            }),
    );
    documents
}

fn exact_reference_search(
    documents: &[ExactReferenceDocument<'_>],
    query: &FixtureQuery,
    top_k: usize,
) -> Vec<RankedDocument> {
    let normalized_query = normalized(&query.embedding);
    let mut hits = documents
        .iter()
        .filter(|document| fixture_filter_matches(query.filter.as_ref(), document.metadata))
        .map(|document| RankedDocument {
            document_id: document.document_id.to_owned(),
            chunk_id: document.chunk_id,
            score: dot_product(&normalized_query, &document.normalized_embedding),
        })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.chunk_id.cmp(&right.chunk_id))
    });
    hits.truncate(top_k);
    hits
}

fn fixture_filter_matches(
    filter: Option<&FixtureFilter>,
    metadata: &BTreeMap<String, String>,
) -> bool {
    filter.is_none_or(|filter| metadata.get(&filter.field) == Some(&filter.value))
}

fn dot_product(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn normalized(vector: &[f32]) -> Vec<f32> {
    let mut normalized = vector.to_vec();
    normalize(&mut normalized);
    normalized
}

fn normalize(vector: &mut [f32]) {
    let squared_norm = vector.iter().map(|value| value * value).sum::<f32>();
    if squared_norm == 0.0 {
        return;
    }
    let inverse_norm = squared_norm.sqrt().recip();
    for value in vector {
        *value *= inverse_norm;
    }
}

fn collect_artifact_hits(
    fixture: &Fixture,
    mut search: impl FnMut(&FixtureQuery) -> Result<Vec<RankedDocument>, String>,
) -> Result<Vec<(String, Vec<RankedDocument>)>, String> {
    fixture
        .queries
        .iter()
        .map(|query| search(query).map(|hits| (query.id.clone(), hits)))
        .collect()
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
        if let Some(minimum) = fixture.quality_gates.min_relevance_recall_at_k {
            if run.relevance_recall_at_k < minimum {
                violations.push(format!(
                    "{} default relevance recall@{} {:.4} is below {:.4}",
                    run.encoding, fixture.top_k, run.relevance_recall_at_k, minimum
                ));
            }
        }
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
    for run in vector_only_runs {
        if let Some(minimum) = fixture.quality_gates.min_relevance_recall_at_k {
            if run.relevance_recall_at_k < minimum {
                violations.push(format!(
                    "{} vector-only relevance recall@{} {:.4} is below {:.4}",
                    run.encoding, run.top_k, run.relevance_recall_at_k, minimum
                ));
            }
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

    #[test]
    fn harder_v2_fixture_passes_quality_gates() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/retrieval-quality/v2/fixture.json");
        let outcome = run(&[
            "--fixture".to_owned(),
            fixture.display().to_string(),
            "--iterations".to_owned(),
            "1".to_owned(),
        ])
        .unwrap();
        assert!(outcome.passed, "{}", outcome.json);
    }

    #[test]
    fn separate_collection_and_qrels_emit_deterministic_artifacts() {
        let directory = TemporaryDirectory::new().unwrap();
        let manifest = directory.path.join("collection.json");
        let documents = directory.path.join("documents.jsonl");
        let queries = directory.path.join("queries.jsonl");
        let qrels = directory.path.join("qrels.tsv");
        fs::write(
            &documents,
            concat!(
                "{\"id\":\"d1\",\"text\":\"alpha\",\"metadata\":{},\"embedding\":[1.0,0.0]}\n",
                "{\"id\":\"d2\",\"text\":\"beta\",\"metadata\":{},\"embedding\":[0.0,1.0]}\n"
            ),
        )
        .unwrap();
        fs::write(
            &queries,
            "{\"id\":\"q1\",\"category\":\"test\",\"text\":\"alpha\",\"embedding\":[1.0,0.0]}\n",
        )
        .unwrap();
        fs::write(&qrels, "q1 0 d1 2\nq1 0 d2 0\n").unwrap();
        fs::write(
            &manifest,
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 2,
                "collection_id": "separate-test",
                "model": {
                    "id": "test",
                    "slug": "test",
                    "sequence_length": 2,
                    "dimension": 2
                },
                "top_k": 1,
                "evaluation_depth": 10,
                "candidate_pairs": [[10, 10]],
                "default_pair": [10, 10],
                "documents_path": "documents.jsonl",
                "queries_path": "queries.jsonl",
                "embedding_provenance": {
                    "generator": "test",
                    "model": "test",
                    "sequence_length": 2,
                    "normalized": true
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let fixture = load_fixture(&manifest, Some(&qrels)).unwrap();
        fixture.validate().unwrap();
        assert_eq!(fixture.queries[0].relevance["d1"], 2);

        let first = directory.path.join("artifacts-a");
        let second = directory.path.join("artifacts-b");
        for artifacts in [&first, &second] {
            let outcome = run(&[
                "--fixture".to_owned(),
                manifest.display().to_string(),
                "--qrels".to_owned(),
                qrels.display().to_string(),
                "--artifacts".to_owned(),
                artifacts.display().to_string(),
                "--iterations".to_owned(),
                "1".to_owned(),
            ])
            .unwrap();
            assert!(outcome.passed, "{}", outcome.json);
        }

        for file in [
            "qrels.tsv",
            "rust-results.json",
            "metrics.json",
            "manifest.json",
        ] {
            assert_eq!(
                fs::read(first.join(file)).unwrap(),
                fs::read(second.join(file)).unwrap()
            );
        }
        let mut run_files = fs::read_dir(first.join("runs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        run_files.sort();
        assert!(!run_files.is_empty());
        for file in run_files {
            assert_eq!(
                fs::read(first.join("runs").join(&file)).unwrap(),
                fs::read(second.join("runs").join(&file)).unwrap()
            );
        }
    }

    fn ranked(document_id: &str, chunk_id: u64) -> RankedDocument {
        RankedDocument {
            document_id: document_id.to_owned(),
            chunk_id,
            score: 0.0,
        }
    }
}
