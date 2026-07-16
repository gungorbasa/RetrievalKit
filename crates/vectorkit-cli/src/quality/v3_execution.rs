use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use vectorkit_core::{HybridHit, HybridQuery, RetrievalDatabase, SearchQuery, VectorEncoding};

use super::v3_canonical::{canonical_json_line, write_canonical_json};
use super::v3_ingestion::{ProductionQueryInput, V3ProductionInputs};
use super::v3_runs::RunIdentity;
use super::v3_schema::Qrel;
use super::v3_validation::ValidatedCollection;

static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);
const QUALIFICATION_SCHEMA: &str = "phase-1.2a-qualification-v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct QualificationResults {
    collection_id: String,
    collection_version: String,
    runs: Vec<RunExecution>,
    schema_version: u8,
    seed_resolutions: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct RunExecution {
    queries: Vec<QueryExecution>,
    run_id: String,
    status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct QueryExecution {
    candidate_limits: CandidateLimits,
    chunk_hits: Vec<ChunkHit>,
    duplicate_collapse_count: usize,
    execution_status: &'static str,
    filter: Option<Value>,
    projected_documents: Vec<ProjectedDocument>,
    query_id: String,
    selection_run_id: Option<String>,
    status_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct CandidateLimits {
    keyword: Option<usize>,
    vector: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct ChunkHit {
    bm25_normalized_score: Option<f32>,
    bm25_score: Option<f32>,
    chunk_key: String,
    fusion_score: Option<f32>,
    keyword_rank: Option<usize>,
    matched_terms: Vec<String>,
    native_rank: usize,
    record_id: String,
    vector_normalized_score: Option<f32>,
    vector_rank: Option<usize>,
    vector_score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct ProjectedDocument {
    chunk_key: String,
    document_rank: usize,
    native_chunk_rank: usize,
    record_id: String,
    score: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
struct ExistingMetrics {
    ap: f64,
    judged_at_10: f64,
    judged_at_5: f64,
    mrr_at_10: f64,
    ndcg_at_10: f64,
    ndcg_at_5: f64,
    precision_at_5: f64,
    recall_at_10: f64,
    recall_at_5: f64,
    success_at_1: f64,
}

impl ExistingMetrics {
    fn add_assign(&mut self, other: Self) {
        self.ap += other.ap;
        self.judged_at_10 += other.judged_at_10;
        self.judged_at_5 += other.judged_at_5;
        self.mrr_at_10 += other.mrr_at_10;
        self.ndcg_at_10 += other.ndcg_at_10;
        self.ndcg_at_5 += other.ndcg_at_5;
        self.precision_at_5 += other.precision_at_5;
        self.recall_at_10 += other.recall_at_10;
        self.recall_at_5 += other.recall_at_5;
        self.success_at_1 += other.success_at_1;
    }

    fn divide_by(&mut self, divisor: f64) {
        self.ap /= divisor;
        self.judged_at_10 /= divisor;
        self.judged_at_5 /= divisor;
        self.mrr_at_10 /= divisor;
        self.ndcg_at_10 /= divisor;
        self.ndcg_at_5 /= divisor;
        self.precision_at_5 /= divisor;
        self.recall_at_10 /= divisor;
        self.recall_at_5 /= divisor;
        self.success_at_1 /= divisor;
    }
}

pub(super) fn execute(validated: &ValidatedCollection) -> Result<QualificationResults, String> {
    let inputs = V3ProductionInputs::from_validated(validated)?;
    let source_queries = validated
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut runs = validated
        .runs
        .iter()
        .filter(|run| {
            matches!(
                run.configuration["run_letter"].as_str(),
                Some("a" | "b" | "c")
            )
        })
        .map(|run| execute_run_with_persistence(run, &inputs, &source_queries, validated))
        .collect::<Result<Vec<_>, _>>()?;
    runs.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    if runs.len() != 3 {
        return Err(format!(
            "V3 Phase 1.2a expected exactly three A-C runs, actual {}",
            runs.len()
        ));
    }
    Ok(QualificationResults {
        collection_id: validated.collection.collection_id.clone(),
        collection_version: validated.collection.collection_version.clone(),
        runs,
        schema_version: 3,
        seed_resolutions: Vec::new(),
    })
}

fn execute_run_with_persistence(
    run: &RunIdentity,
    inputs: &V3ProductionInputs,
    source_queries: &BTreeMap<&str, &super::v3_schema::Query>,
    validated: &ValidatedCollection,
) -> Result<RunExecution, String> {
    let encoding = match run.configuration["vector_encoding"].as_str() {
        Some("f32") => VectorEncoding::F32,
        Some("i8") => VectorEncoding::I8ScalarQuantized,
        actual => {
            return Err(format!(
                "V3 Phase 1.2a run '{}' has unsupported vector encoding {actual:?}",
                run.run_id
            ));
        }
    };
    let database = inputs.build_database(encoding)?;
    let before = execute_run(run, inputs, source_queries, validated, &database)?;

    let temporary = TemporaryDirectory::new("vectorkit-v3-phase-1-2a-persistence")?;
    let persisted = temporary.path.join("database");
    database
        .save_to_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2a save '{}': {error}", run.run_id))?;
    RetrievalDatabase::validate_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2a validate '{}': {error}", run.run_id))?;
    let loaded = RetrievalDatabase::load_from_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2a reload '{}': {error}", run.run_id))?;
    verify_persisted_database(&database, &loaded, run)?;
    let after = execute_run(run, inputs, source_queries, validated, &loaded)?;
    if before != after {
        return Err(format!(
            "V3 Phase 1.2a persistence_mismatch for run '{}'",
            run.run_id
        ));
    }
    Ok(before)
}

fn verify_persisted_database(
    before: &RetrievalDatabase,
    after: &RetrievalDatabase,
    run: &RunIdentity,
) -> Result<(), String> {
    let before_identities = before
        .corpus()
        .chunk_identities()
        .map(|(identity, chunk_id)| {
            (
                identity.record_id.as_str().to_owned(),
                identity.chunk_key.as_str().to_owned(),
                chunk_id,
            )
        })
        .collect::<Vec<_>>();
    let after_identities = after
        .corpus()
        .chunk_identities()
        .map(|(identity, chunk_id)| {
            (
                identity.record_id.as_str().to_owned(),
                identity.chunk_key.as_str().to_owned(),
                chunk_id,
            )
        })
        .collect::<Vec<_>>();
    if before.corpus().generation() != after.corpus().generation()
        || before.retrieval().vector_encoding() != after.retrieval().vector_encoding()
        || before.retrieval().dimension() != after.retrieval().dimension()
        || before.retrieval().metric() != after.retrieval().metric()
        || before_identities != after_identities
    {
        return Err(format!(
            "V3 Phase 1.2a reload_mismatch for run '{}'",
            run.run_id
        ));
    }
    Ok(())
}

fn execute_run(
    run: &RunIdentity,
    inputs: &V3ProductionInputs,
    source_queries: &BTreeMap<&str, &super::v3_schema::Query>,
    validated: &ValidatedCollection,
    database: &RetrievalDatabase,
) -> Result<RunExecution, String> {
    let letter = run.configuration["run_letter"]
        .as_str()
        .ok_or_else(|| format!("V3 Phase 1.2a run '{}' has no run letter", run.run_id))?;
    let candidate_limits = candidate_limits(run)?;
    let evaluation_depth = validated.collection.evaluation_depth;
    let mut queries = Vec::with_capacity(run.execution.len());
    for query_id in &run.execution {
        let input = inputs
            .queries
            .iter()
            .find(|query| query.query_id == *query_id)
            .ok_or_else(|| {
                format!(
                    "V3 Phase 1.2a run '{}' is missing query '{}'",
                    run.run_id, query_id
                )
            })?;
        let source = source_queries
            .get(query_id.as_str())
            .ok_or_else(|| format!("V3 Phase 1.2a source query '{}' is missing", query_id))?;
        let chunk_hits = if matches!(letter, "a" | "b") {
            semantic_hits(database, input)?
        } else if letter == "c" {
            let alpha = run.configuration["fusion_alpha"]
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                .ok_or_else(|| {
                    format!(
                        "V3 Phase 1.2a run '{}' has invalid fusion alpha",
                        run.run_id
                    )
                })?;
            hybrid_hits(database, input, candidate_limits, alpha)?
        } else {
            return Err(format!(
                "V3 Phase 1.2a attempted non-A-C run '{}'",
                run.run_id
            ));
        };
        validate_native_hits(&chunk_hits, run, query_id)?;
        let (projected_documents, duplicate_collapse_count) =
            project_documents(&chunk_hits, evaluation_depth);
        queries.push(QueryExecution {
            candidate_limits,
            chunk_hits,
            duplicate_collapse_count,
            execution_status: "valid",
            filter: source.metadata_filter.clone(),
            projected_documents,
            query_id: query_id.clone(),
            selection_run_id: None,
            status_reason: None,
        });
    }
    queries.sort_by(|left, right| left.query_id.cmp(&right.query_id));
    Ok(RunExecution {
        queries,
        run_id: run.run_id.clone(),
        status: "valid",
    })
}

fn candidate_limits(run: &RunIdentity) -> Result<CandidateLimits, String> {
    let read = |name: &str| {
        let value = &run.configuration["candidate_limits"][name];
        if value.is_null() {
            Ok(None)
        } else {
            value
                .as_u64()
                .map(|value| Some(value as usize))
                .ok_or_else(|| {
                    format!(
                        "V3 Phase 1.2a run '{}' has invalid {name} candidate limit",
                        run.run_id
                    )
                })
        }
    };
    Ok(CandidateLimits {
        keyword: read("keyword")?,
        vector: read("vector")?,
    })
}

fn semantic_hits(
    database: &RetrievalDatabase,
    query: &ProductionQueryInput,
) -> Result<Vec<ChunkHit>, String> {
    let mut request = SearchQuery::new(
        query.embedding.clone(),
        database.corpus().active_chunk_count(),
    );
    if let Some(filter) = &query.filter {
        request = request.with_filter(filter.clone());
    }
    database
        .semantic_search(&request)
        .map_err(|error| format!("V3 Phase 1.2a semantic query '{}': {error}", query.query_id))?
        .into_iter()
        .enumerate()
        .map(|(offset, hit)| {
            let identity = database
                .corpus()
                .chunk_identity(hit.chunk_id)
                .ok_or_else(|| {
                    format!(
                        "V3 Phase 1.2a semantic query '{}' returned unknown chunk {}",
                        query.query_id, hit.chunk_id
                    )
                })?;
            if hit.document_id != identity.record_id.as_str() || !hit.score.is_finite() {
                return Err(format!(
                    "V3 Phase 1.2a semantic query '{}' returned invalid identity or score",
                    query.query_id
                ));
            }
            Ok(ChunkHit {
                bm25_normalized_score: None,
                bm25_score: None,
                chunk_key: identity.chunk_key.as_str().to_owned(),
                fusion_score: None,
                keyword_rank: None,
                matched_terms: Vec::new(),
                native_rank: offset + 1,
                record_id: identity.record_id.as_str().to_owned(),
                vector_normalized_score: None,
                vector_rank: Some(offset + 1),
                vector_score: Some(hit.score),
            })
        })
        .collect()
}

fn hybrid_hits(
    database: &RetrievalDatabase,
    query: &ProductionQueryInput,
    limits: CandidateLimits,
    alpha: f32,
) -> Result<Vec<ChunkHit>, String> {
    let vector_limit = limits
        .vector
        .ok_or_else(|| "V3 Phase 1.2a weighted run lacks vector limit".to_owned())?;
    let keyword_limit = limits
        .keyword
        .ok_or_else(|| "V3 Phase 1.2a weighted run lacks keyword limit".to_owned())?;
    let mut request = HybridQuery::new(
        query.text.clone(),
        query.embedding.clone(),
        vector_limit.saturating_add(keyword_limit),
    )
    .with_candidate_limits(vector_limit, keyword_limit)
    .with_alpha(alpha);
    if let Some(filter) = &query.filter {
        request = request.with_filter(filter.clone());
    }
    database
        .hybrid_search(&request)
        .map_err(|error| format!("V3 Phase 1.2a hybrid query '{}': {error}", query.query_id))?
        .into_iter()
        .enumerate()
        .map(|(offset, hit)| convert_hybrid_hit(database, query, offset, hit))
        .collect()
}

fn convert_hybrid_hit(
    database: &RetrievalDatabase,
    query: &ProductionQueryInput,
    offset: usize,
    hit: HybridHit,
) -> Result<ChunkHit, String> {
    let identity = database
        .corpus()
        .chunk_identity(hit.chunk_id)
        .ok_or_else(|| {
            format!(
                "V3 Phase 1.2a hybrid query '{}' returned unknown chunk {}",
                query.query_id, hit.chunk_id
            )
        })?;
    if hit.document_id != identity.record_id.as_str()
        || !hit.score.is_finite()
        || hit.vector_score.is_some_and(|score| !score.is_finite())
        || hit.keyword_score.is_some_and(|score| !score.is_finite())
        || hit
            .trace
            .normalized_vector_score
            .is_some_and(|score| !score.is_finite())
        || hit
            .trace
            .normalized_keyword_score
            .is_some_and(|score| !score.is_finite())
    {
        return Err(format!(
            "V3 Phase 1.2a hybrid query '{}' returned invalid identity or score",
            query.query_id
        ));
    }
    if hit.vector_score.is_some() != hit.trace.normalized_vector_score.is_some()
        || hit.keyword_score.is_some() != hit.trace.normalized_keyword_score.is_some()
        || hit.vector_score.is_some() != hit.trace.vector_rank.is_some()
        || hit.keyword_score.is_some() != hit.trace.keyword_rank.is_some()
        || (hit.trace.keyword_rank.is_none() && !hit.trace.matched_terms.is_empty())
    {
        return Err(format!(
            "V3 Phase 1.2a hybrid query '{}' returned inconsistent production trace",
            query.query_id
        ));
    }
    Ok(ChunkHit {
        bm25_normalized_score: hit.trace.normalized_keyword_score,
        bm25_score: hit.keyword_score,
        chunk_key: identity.chunk_key.as_str().to_owned(),
        fusion_score: Some(hit.score),
        keyword_rank: hit.trace.keyword_rank,
        matched_terms: hit.trace.matched_terms,
        native_rank: offset + 1,
        record_id: identity.record_id.as_str().to_owned(),
        vector_normalized_score: hit.trace.normalized_vector_score,
        vector_rank: hit.trace.vector_rank,
        vector_score: hit.vector_score,
    })
}

fn validate_native_hits(
    hits: &[ChunkHit],
    run: &RunIdentity,
    query_id: &str,
) -> Result<(), String> {
    let mut identities = BTreeSet::new();
    for (offset, hit) in hits.iter().enumerate() {
        if hit.native_rank != offset + 1
            || !identities.insert((hit.record_id.as_str(), hit.chunk_key.as_str()))
        {
            return Err(format!(
                "V3 Phase 1.2a contract_violation in run '{}' query '{}': duplicate or non-consecutive native hit",
                run.run_id, query_id
            ));
        }
    }
    for pair in hits.windows(2) {
        let left_score = pair[0].fusion_score.or(pair[0].vector_score).unwrap();
        let right_score = pair[1].fusion_score.or(pair[1].vector_score).unwrap();
        let left_identity = (&pair[0].record_id, &pair[0].chunk_key);
        let right_identity = (&pair[1].record_id, &pair[1].chunk_key);
        if left_score.total_cmp(&right_score).is_lt()
            || (left_score == right_score && left_identity >= right_identity)
        {
            return Err(format!(
                "V3 Phase 1.2a contract_violation in run '{}' query '{}': native ordering or stable tie-break mismatch",
                run.run_id, query_id
            ));
        }
    }
    Ok(())
}

fn project_documents(
    hits: &[ChunkHit],
    evaluation_depth: usize,
) -> (Vec<ProjectedDocument>, usize) {
    let mut seen = BTreeSet::new();
    let mut projected = Vec::new();
    let mut duplicates = 0;
    for hit in hits {
        if projected.len() == evaluation_depth {
            break;
        }
        if !seen.insert(hit.record_id.as_str()) {
            duplicates += 1;
            continue;
        }
        projected.push(ProjectedDocument {
            chunk_key: hit.chunk_key.clone(),
            document_rank: projected.len() + 1,
            native_chunk_rank: hit.native_rank,
            record_id: hit.record_id.clone(),
            score: hit.fusion_score.or(hit.vector_score).unwrap(),
        });
    }
    (projected, duplicates)
}

pub(super) fn emit_qualification(
    validated: &ValidatedCollection,
    output: &Path,
) -> Result<QualificationResults, String> {
    if output.exists() {
        return Err(format!(
            "Phase 1.2a qualification root '{}' already exists; a fresh directory is required",
            output.display()
        ));
    }
    let results = execute(validated)?;
    fs::create_dir_all(output.join("runs")).map_err(|error| {
        format!(
            "failed to create Phase 1.2a qualification root '{}': {error}",
            output.display()
        )
    })?;
    fs::write(output.join("qrels.tsv"), &validated.bytes["qrels.tsv"])
        .map_err(|error| format!("failed to write Phase 1.2a qrels: {error}"))?;
    write_canonical_json(
        &output.join("rust-results.json"),
        &serde_json::to_value(&results)
            .map_err(|error| format!("failed to encode Phase 1.2a Rust results: {error}"))?,
    )?;
    write_canonical_json(
        &output.join("metrics.json"),
        &metrics_artifact(validated, &results),
    )?;
    fs::write(
        output.join("timing-samples.jsonl"),
        canonical_json_line(&json!({"profile":"deterministic_quality","status":"not_measured"}))?,
    )
    .map_err(|error| format!("failed to write Phase 1.2a timing marker: {error}"))?;
    for run in &results.runs {
        fs::write(
            output.join("runs").join(format!("{}.trec", run.run_id)),
            trec(run, validated.collection.evaluation_depth),
        )
        .map_err(|error| {
            format!(
                "failed to write Phase 1.2a TREC run '{}': {error}",
                run.run_id
            )
        })?;
    }
    super::v3_graph_execution::emit_graph_qualification(validated, output)?;
    write_canonical_json(
        &output.join("qualification.json"),
        &json!({
            "artifact_schema":"phase-1.2b-qualification-v1",
            "collection_id":validated.collection.collection_id,
            "collection_version":validated.collection.collection_version,
            "included_run_letters":["a","b","c","d"],
            "partial":true,
            "publication_ready":false,
            "status":"qualification_only_no_final_manifest"
        }),
    )?;
    Ok(results)
}

pub(super) fn verify_qualification_deterministic_rerun(
    validated: &ValidatedCollection,
) -> Result<(), String> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| format!("failed to resolve repository root: {error}"))?;
    let target = repository.join("target/benchmarks/v3");
    fs::create_dir_all(&target).map_err(|error| {
        format!(
            "failed to create Phase 1.2a rerun root '{}': {error}",
            target.display()
        )
    })?;
    let first = TemporaryDirectory::new_in(&target, "phase-1.2a-rerun-a")?;
    let second = TemporaryDirectory::new_in(&target, "phase-1.2a-rerun-b")?;
    let first_output = first.path.join("qualification");
    let second_output = second.path.join("qualification");
    emit_qualification(validated, &first_output)?;
    emit_qualification(validated, &second_output)?;
    super::v3::compare_directories_with_label(
        &first_output,
        &second_output,
        "Phase 1.2a qualification",
    )
}

fn trec(run: &RunExecution, evaluation_depth: usize) -> Vec<u8> {
    let mut output = String::new();
    for query in &run.queries {
        for document in &query.projected_documents {
            let score = evaluation_depth - document.document_rank + 1;
            output.push_str(&format!(
                "{} Q0 {} {} {} {}\n",
                query.query_id, document.record_id, document.document_rank, score, run.run_id
            ));
        }
    }
    output.into_bytes()
}

fn metrics_artifact(validated: &ValidatedCollection, results: &QualificationResults) -> Value {
    let qrels = qrels_by_query(&validated.qrels);
    let runs = results
        .runs
        .iter()
        .map(|run| {
            let mut macro_metrics = ExistingMetrics::default();
            let queries = run
                .queries
                .iter()
                .map(|query| {
                    let metrics = query_metrics(
                        &query.projected_documents,
                        qrels.get(query.query_id.as_str()).unwrap(),
                    );
                    macro_metrics.add_assign(metrics);
                    json!({"execution_status":"valid","metrics":metrics,"query_id":query.query_id})
                })
                .collect::<Vec<_>>();
            macro_metrics.divide_by(queries.len() as f64);
            let identity = validated
                .runs
                .iter()
                .find(|candidate| candidate.run_id == run.run_id)
                .unwrap();
            json!({
                "declared_population_sha256":identity.declared_hash(),
                "execution_population_sha256":identity.execution_hash(),
                "macro":macro_metrics,
                "queries":queries,
                "run_id":run.run_id,
                "status":"valid"
            })
        })
        .collect::<Vec<_>>();
    json!({
        "artifact_schema":QUALIFICATION_SCHEMA,
        "collection_id":validated.collection.collection_id,
        "collection_version":validated.collection.collection_version,
        "metric_definition_version":"graph-retrieval-v3-r2-existing-retrieval-subset",
        "partial":true,
        "publication_ready":false,
        "runs":runs
    })
}

fn qrels_by_query(qrels: &[Qrel]) -> BTreeMap<&str, BTreeMap<&str, u8>> {
    let mut values = BTreeMap::new();
    for qrel in qrels {
        values
            .entry(qrel.query_id.as_str())
            .or_insert_with(BTreeMap::new)
            .insert(qrel.record_id.as_str(), qrel.relevance);
    }
    values
}

fn query_metrics(hits: &[ProjectedDocument], qrels: &BTreeMap<&str, u8>) -> ExistingMetrics {
    ExistingMetrics {
        ap: average_precision(hits, qrels),
        judged_at_10: judged(hits, qrels, 10),
        judged_at_5: judged(hits, qrels, 5),
        mrr_at_10: reciprocal_rank(hits, qrels, 10),
        ndcg_at_10: ndcg(hits, qrels, 10),
        ndcg_at_5: ndcg(hits, qrels, 5),
        precision_at_5: relevant_count(hits, qrels, 5) as f64 / 5.0,
        recall_at_10: recall(hits, qrels, 10),
        recall_at_5: recall(hits, qrels, 5),
        success_at_1: f64::from(relevant_count(hits, qrels, 1) > 0),
    }
}

fn relevant_count(hits: &[ProjectedDocument], qrels: &BTreeMap<&str, u8>, cutoff: usize) -> usize {
    hits.iter()
        .take(cutoff)
        .filter(|hit| {
            qrels
                .get(hit.record_id.as_str())
                .is_some_and(|grade| *grade >= 1)
        })
        .count()
}

fn recall(hits: &[ProjectedDocument], qrels: &BTreeMap<&str, u8>, cutoff: usize) -> f64 {
    let relevant = qrels.values().filter(|grade| **grade >= 1).count();
    relevant_count(hits, qrels, cutoff) as f64 / relevant as f64
}

fn reciprocal_rank(hits: &[ProjectedDocument], qrels: &BTreeMap<&str, u8>, cutoff: usize) -> f64 {
    hits.iter()
        .take(cutoff)
        .position(|hit| {
            qrels
                .get(hit.record_id.as_str())
                .is_some_and(|grade| *grade >= 1)
        })
        .map(|offset| 1.0 / (offset + 1) as f64)
        .unwrap_or(0.0)
}

fn average_precision(hits: &[ProjectedDocument], qrels: &BTreeMap<&str, u8>) -> f64 {
    let relevant = qrels.values().filter(|grade| **grade >= 1).count();
    let mut found = 0_usize;
    let mut sum = 0.0_f64;
    for (offset, hit) in hits.iter().enumerate() {
        if qrels
            .get(hit.record_id.as_str())
            .is_some_and(|grade| *grade >= 1)
        {
            found += 1;
            sum += found as f64 / (offset + 1) as f64;
        }
    }
    sum / relevant as f64
}

fn judged(hits: &[ProjectedDocument], qrels: &BTreeMap<&str, u8>, cutoff: usize) -> f64 {
    let denominator = cutoff.min(hits.len());
    if denominator == 0 {
        return 0.0;
    }
    hits.iter()
        .take(cutoff)
        .filter(|hit| qrels.contains_key(hit.record_id.as_str()))
        .count() as f64
        / denominator as f64
}

fn ndcg(hits: &[ProjectedDocument], qrels: &BTreeMap<&str, u8>, cutoff: usize) -> f64 {
    let mut dcg = 0.0_f64;
    for (offset, hit) in hits.iter().take(cutoff).enumerate() {
        dcg += gain(*qrels.get(hit.record_id.as_str()).unwrap_or(&0), offset);
    }
    let mut ideal = qrels.iter().collect::<Vec<_>>();
    ideal.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let mut idcg = 0.0_f64;
    for (offset, (_, grade)) in ideal.into_iter().take(cutoff).enumerate() {
        idcg += gain(*grade, offset);
    }
    dcg / idcg
}

fn gain(grade: u8, offset: usize) -> f64 {
    let exact_gain = (1_u128 << grade) - 1;
    exact_gain as f64 / (offset as f64 + 2.0).log2()
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(prefix: &str) -> Result<Self, String> {
        Self::new_in(&std::env::temp_dir(), prefix)
    }

    fn new_in(parent: &Path, prefix: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_nanos();
        let counter = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!("{prefix}-{}-{nonce}-{counter}", std::process::id()));
        fs::create_dir(&path).map_err(|error| {
            format!(
                "failed to create persistence directory '{}': {error}",
                path.display()
            )
        })?;
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
    use crate::quality::v3_validation::validate;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    #[test]
    fn executes_a_through_c_with_complete_native_hits_projection_and_reload_equivalence() {
        let validated = validate(&fixture_root()).unwrap();
        let results = execute(&validated).unwrap();

        assert_eq!(results.runs.len(), 3);
        for run in &results.runs {
            assert_eq!(run.queries.len(), 7);
            assert!(run
                .queries
                .iter()
                .all(|query| query.execution_status == "valid"));
            let qa = run
                .queries
                .iter()
                .find(|query| query.query_id == "qa")
                .unwrap();
            assert_eq!(qa.chunk_hits.len(), 8);
            assert_eq!(qa.projected_documents.len(), 7);
            assert_eq!(qa.duplicate_collapse_count, 1);
            let qb = run
                .queries
                .iter()
                .find(|query| query.query_id == "qb")
                .unwrap();
            assert_eq!(qb.chunk_hits.len(), 4);
            assert!(qb.chunk_hits.iter().all(|hit| {
                matches!(hit.record_id.as_str(), "alpha" | "beta" | "gamma" | "phone")
            }));
            let qi = run
                .queries
                .iter()
                .find(|query| query.query_id == "qi")
                .unwrap();
            assert_eq!(qi.chunk_hits.len(), 4);
            assert!(qi.chunk_hits.iter().all(|hit| {
                matches!(
                    hit.record_id.as_str(),
                    "alpha" | "mobile" | "shared-east" | "shared-west"
                )
            }));
        }
    }

    #[test]
    fn projection_counts_only_duplicates_scanned_before_depth_or_exhaustion() {
        let hits = [
            test_hit("a", "one", 1),
            test_hit("a", "two", 2),
            test_hit("b", "one", 3),
            test_hit("b", "two", 4),
        ];
        let (projected, duplicates) = project_documents(&hits, 2);
        assert_eq!(
            projected
                .iter()
                .map(|hit| hit.record_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(duplicates, 1);
    }

    #[test]
    fn metric_formulas_use_fixed_precision_cutoff_and_rank_order() {
        let hits = [
            ProjectedDocument {
                chunk_key: "one".to_owned(),
                document_rank: 1,
                native_chunk_rank: 1,
                record_id: "positive".to_owned(),
                score: 1.0,
            },
            ProjectedDocument {
                chunk_key: "one".to_owned(),
                document_rank: 2,
                native_chunk_rank: 2,
                record_id: "zero".to_owned(),
                score: 0.0,
            },
        ];
        let qrels = BTreeMap::from([("positive", 2), ("zero", 0)]);
        let metrics = query_metrics(&hits, &qrels);
        assert_eq!(metrics.precision_at_5, 0.2);
        assert_eq!(metrics.recall_at_5, 1.0);
        assert_eq!(metrics.ap, 1.0);
        assert_eq!(metrics.judged_at_5, 1.0);
        assert_eq!(metrics.ndcg_at_5, 1.0);
    }

    #[test]
    fn emits_only_partial_qualification_artifacts_without_a_final_manifest() {
        let validated = validate(&fixture_root()).unwrap();
        let temporary = TemporaryDirectory::new("vectorkit-v3-phase-1-2a-artifacts").unwrap();
        let output = temporary.path.join("qualification");

        emit_qualification(&validated, &output).unwrap();

        assert_eq!(
            fs::read(output.join("qrels.tsv")).unwrap(),
            validated.bytes["qrels.tsv"]
        );
        assert!(output.join("rust-results.json").is_file());
        assert!(output.join("metrics.json").is_file());
        assert!(output.join("qualification.json").is_file());
        assert!(!output.join("manifest.json").exists());
        let marker: Value =
            serde_json::from_slice(&fs::read(output.join("qualification.json")).unwrap()).unwrap();
        assert_eq!(marker["partial"], true);
        assert_eq!(marker["publication_ready"], false);
        assert_eq!(fs::read_dir(output.join("runs")).unwrap().count(), 3);
    }

    #[test]
    fn phase_1_2a_qualification_rerun_is_byte_identical() {
        let validated = validate(&fixture_root()).unwrap();
        verify_qualification_deterministic_rerun(&validated).unwrap();
    }

    fn test_hit(record_id: &str, chunk_key: &str, native_rank: usize) -> ChunkHit {
        ChunkHit {
            bm25_normalized_score: None,
            bm25_score: None,
            chunk_key: chunk_key.to_owned(),
            fusion_score: None,
            keyword_rank: None,
            matched_terms: Vec::new(),
            native_rank,
            record_id: record_id.to_owned(),
            vector_normalized_score: None,
            vector_rank: Some(native_rank),
            vector_score: Some(1.0 / native_rank as f32),
        }
    }
}
