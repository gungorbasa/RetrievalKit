use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use vectorkit_core::{HybridHit, HybridQuery, RetrievalDatabase, SearchQuery, VectorEncoding};

use super::v3_canonical::{canonical_json, canonical_json_line, sha256, write_canonical_json};
use super::v3_execution_status::{ExecutionFailures, FailureReason};
use super::v3_ingestion::{ProductionQueryInput, V3ProductionInputs};
use super::v3_runs::{
    canonical_runs_with_hybrid_configuration, HybridConfiguration, RunContext, RunIdentity,
};
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

impl QualificationResults {
    pub(super) fn has_invalid_execution(&self) -> bool {
        self.runs
            .iter()
            .any(|run| run.status == "invalid_execution")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompleteQualificationStatus {
    pub(super) phase_1_2a: &'static str,
    pub(super) phase_1_2b: &'static str,
    pub(super) phase_1_2c: &'static str,
    pub(super) qualification: &'static str,
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

#[cfg(test)]
pub(super) fn execute(validated: &ValidatedCollection) -> Result<QualificationResults, String> {
    execute_with_failures(validated, &ExecutionFailures::default())
}

fn execute_with_failures(
    validated: &ValidatedCollection,
    failures: &ExecutionFailures,
) -> Result<QualificationResults, String> {
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
        .map(|run| execute_run_with_persistence(run, &inputs, &source_queries, validated, failures))
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
    injected_failures: &ExecutionFailures,
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
    let mut before = execute_run(run, inputs, source_queries, validated, &database)?;
    let repeated = execute_run(run, inputs, source_queries, validated, &database)?;
    let mut failures = injected_failures.clone();
    if before != repeated {
        failures.run(run.run_id.clone(), FailureReason::NonDeterministicRanking);
    }

    let temporary = TemporaryDirectory::new("vectorkit-v3-phase-1-2a-persistence")?;
    let persisted = temporary.path.join("database");
    database
        .save_to_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2a save '{}': {error}", run.run_id))?;
    RetrievalDatabase::validate_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2a validate '{}': {error}", run.run_id))?;
    let loaded = RetrievalDatabase::load_from_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2a reload '{}': {error}", run.run_id))?;
    if verify_persisted_database(&database, &loaded, run).is_err() {
        failures.run(run.run_id.clone(), FailureReason::ReloadMismatch);
    }
    let after = execute_run(run, inputs, source_queries, validated, &loaded)?;
    if before != after {
        failures.run(run.run_id.clone(), FailureReason::PersistenceMismatch);
    }
    apply_failures(&mut before, &failures);
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
        let execution = (|| {
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
            Ok(QueryExecution {
                candidate_limits,
                chunk_hits,
                duplicate_collapse_count,
                execution_status: "valid",
                filter: source.metadata_filter.clone(),
                projected_documents,
                query_id: query_id.clone(),
                selection_run_id: None,
                status_reason: None,
            })
        })();
        queries.push(execution.unwrap_or_else(|_| QueryExecution {
            candidate_limits,
            chunk_hits: Vec::new(),
            duplicate_collapse_count: 0,
            execution_status: "invalid_execution",
            filter: source.metadata_filter.clone(),
            projected_documents: Vec::new(),
            query_id: query_id.clone(),
            selection_run_id: None,
            status_reason: Some(FailureReason::ContractViolation.as_str().to_owned()),
        }));
    }
    queries.sort_by(|left, right| left.query_id.cmp(&right.query_id));
    let status = if queries
        .iter()
        .any(|query| query.execution_status == "invalid_execution")
    {
        "invalid_execution"
    } else {
        "valid"
    };
    Ok(RunExecution {
        queries,
        run_id: run.run_id.clone(),
        status,
    })
}

fn apply_failures(run: &mut RunExecution, failures: &ExecutionFailures) {
    for query in &mut run.queries {
        if let Some(reason) = failures.reason_for(&run.run_id, &query.query_id) {
            query.execution_status = "invalid_execution";
            query.status_reason = Some(reason.as_str().to_owned());
            query.chunk_hits.clear();
            query.projected_documents.clear();
            query.duplicate_collapse_count = 0;
        }
    }
    run.status = if run
        .queries
        .iter()
        .any(|query| query.execution_status == "invalid_execution")
    {
        "invalid_execution"
    } else {
        "valid"
    };
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

pub(super) fn emit_hotpotqa_tuning_search(
    validated: &ValidatedCollection,
    candidates: &[HybridConfiguration],
    context: &RunContext,
    search_space_sha256: &str,
    output: &Path,
) -> Result<Value, String> {
    if output.exists() {
        return Err(format!(
            "HotpotQA tuning root '{}' already exists; a fresh directory is required",
            output.display()
        ));
    }
    if candidates.is_empty() {
        return Err("HotpotQA tuning search space is empty".to_owned());
    }
    let parent = output
        .parent()
        .ok_or_else(|| format!("HotpotQA tuning root '{}' has no parent", output.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create HotpotQA tuning parent: {error}"))?;
    let staging = TemporaryDirectory::new_in(parent, ".hotpotqa-phase-3-tuning-staging")?;
    let staged_output = &staging.path;
    fs::create_dir_all(staged_output.join("candidates"))
        .map_err(|error| format!("create HotpotQA candidate root: {error}"))?;

    let inputs = V3ProductionInputs::from_validated(validated)?;
    let source_queries = validated
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let database = inputs.build_database(VectorEncoding::I8ScalarQuantized)?;

    let mut candidate_runs = Vec::with_capacity(candidates.len());
    let mut seen = BTreeSet::new();
    for candidate in candidates {
        let runs = canonical_runs_with_hybrid_configuration(
            &validated.collection,
            &validated.queries,
            &validated.populations,
            context,
            *candidate,
        )?;
        let run = runs
            .into_iter()
            .find(|run| run.configuration["run_letter"] == "c")
            .ok_or_else(|| "HotpotQA tuning candidate did not produce Run C".to_owned())?;
        if !seen.insert(run.run_id.clone()) {
            return Err(format!(
                "HotpotQA tuning search produced duplicate run ID '{}'",
                run.run_id
            ));
        }
        let before = execute_run(&run, &inputs, &source_queries, validated, &database)?;
        let repeated = execute_run(&run, &inputs, &source_queries, validated, &database)?;
        if before != repeated {
            return Err(format!(
                "HotpotQA tuning run '{}' was not deterministic before persistence",
                run.run_id
            ));
        }
        candidate_runs.push((*candidate, run, before));
    }

    let temporary = TemporaryDirectory::new("vectorkit-hotpotqa-phase-3-tuning-persistence")?;
    let persisted = temporary.path.join("database");
    database
        .save_to_dir(&persisted)
        .map_err(|error| format!("HotpotQA tuning database save: {error}"))?;
    RetrievalDatabase::validate_dir(&persisted)
        .map_err(|error| format!("HotpotQA tuning database validation: {error}"))?;
    let loaded = RetrievalDatabase::load_from_dir(&persisted)
        .map_err(|error| format!("HotpotQA tuning database reload: {error}"))?;

    let mut summaries = Vec::with_capacity(candidate_runs.len());
    for (candidate, run, before) in candidate_runs {
        verify_persisted_database(&database, &loaded, &run)?;
        let after = execute_run(&run, &inputs, &source_queries, validated, &loaded)?;
        if before != after {
            return Err(format!(
                "HotpotQA tuning run '{}' changed after persistence reload",
                run.run_id
            ));
        }
        if before.status != "valid" || before.queries.len() != run.execution.len() {
            return Err(format!(
                "HotpotQA tuning run '{}' has invalid or incomplete execution",
                run.run_id
            ));
        }

        let candidate_output = staged_output.join("candidates").join(&run.run_id);
        fs::create_dir(&candidate_output)
            .map_err(|error| format!("create candidate output '{}': {error}", run.run_id))?;
        let candidate_value = json!({
            "fusion_alpha":candidate.fusion_alpha,
            "keyword_candidate_limit":candidate.keyword_candidate_limit,
            "vector_candidate_limit":candidate.vector_candidate_limit
        });
        let metrics = hotpotqa_tuning_metrics(validated, &before)?;
        let configuration_sha256 = sha256(run.configuration_preimage.as_bytes());
        write_canonical_json(
            &candidate_output.join("configuration.json"),
            &json!({
                "candidate":candidate_value,
                "configuration":run.configuration,
                "configuration_preimage":run.configuration_preimage,
                "configuration_preimage_sha256":configuration_sha256,
                "run_id":run.run_id,
                "schema_version":1
            }),
        )?;
        write_canonical_json(
            &candidate_output.join("rust-results.json"),
            &json!({
                "collection_id":validated.collection.collection_id,
                "collection_version":validated.collection.collection_version,
                "runs":[before],
                "schema_version":3,
                "seed_resolutions":[]
            }),
        )?;
        fs::write(
            candidate_output.join("run.trec"),
            trec(&before, validated.collection.evaluation_depth),
        )
        .map_err(|error| format!("write tuning TREC '{}': {error}", run.run_id))?;
        write_canonical_json(&candidate_output.join("metrics.json"), &metrics)?;
        write_canonical_json(
            &candidate_output.join("persistence.json"),
            &json!({
                "database_shared_across_registered_candidates":true,
                "deterministic_repeat_equal":true,
                "invalid_execution":false,
                "ranking_equal_after_reload":true,
                "run_id":run.run_id,
                "save_validate_load_equivalent":true,
                "schema_version":1
            }),
        )?;
        let files = tuning_file_inventory(&candidate_output, true)?;
        write_canonical_json(
            &candidate_output.join("manifest.json"),
            &json!({
                "files":files,
                "run_id":run.run_id,
                "schema_version":1,
                "status":"valid"
            }),
        )?;
        summaries.push(json!({
            "aggregate":metrics["aggregate"],
            "candidate":candidate_value,
            "configuration_preimage_sha256":configuration_sha256,
            "run_id":run.run_id
        }));
    }
    summaries.sort_by(tuning_candidate_order);
    let winner = summaries
        .first()
        .cloned()
        .ok_or_else(|| "HotpotQA tuning search produced no winner".to_owned())?;
    let tie_break_trace = summaries
        .iter()
        .enumerate()
        .map(|(offset, summary)| {
            json!({
                "candidate":summary["candidate"],
                "objective":[
                    summary["aggregate"]["complete_evidence_recall_at_10"].clone(),
                    summary["aggregate"]["ndcg_at_10"].clone(),
                    summary["aggregate"]["map"].clone(),
                    summary["aggregate"]["recall_at_10"].clone(),
                    summary["aggregate"]["mrr_at_10"].clone(),
                    summary["candidate"]["vector_candidate_limit"].as_u64().unwrap()
                        + summary["candidate"]["keyword_candidate_limit"].as_u64().unwrap(),
                    summary["candidate"]["vector_candidate_limit"].as_u64().unwrap().max(
                        summary["candidate"]["keyword_candidate_limit"].as_u64().unwrap()
                    ),
                    canonical_json(&summary["candidate"]).expect("candidate is canonicalizable")
                ],
                "rank":offset+1,
                "run_id":summary["run_id"]
            })
        })
        .collect::<Vec<_>>();
    let provisional = json!({
        "candidate_count":summaries.len(),
        "collection_id":validated.collection.collection_id,
        "development_population_sha256":run_population_hash(validated),
        "run_c_alone_selected":true,
        "schema_version":1,
        "search_space_sha256":search_space_sha256,
        "selected":winner,
        "selection_objective":[
            "complete_evidence_recall_at_10_desc",
            "ndcg_at_10_desc",
            "map_desc",
            "recall_at_10_desc",
            "mrr_at_10_desc",
            "total_candidate_count_asc",
            "maximum_component_candidate_count_asc",
            "canonical_configuration_bytes_asc"
        ],
        "test_results_available":false,
        "tie_break_trace":tie_break_trace
    });
    write_canonical_json(
        &staged_output.join("selected-configuration-provisional.json"),
        &provisional,
    )?;
    write_canonical_json(
        &staged_output.join("tuning-summary.json"),
        &json!({
            "candidates":summaries,
            "collection_id":validated.collection.collection_id,
            "schema_version":1,
            "search_space_sha256":search_space_sha256,
            "status":"valid"
        }),
    )?;
    let root_files = tuning_file_inventory(staged_output, false)?;
    write_canonical_json(
        &staged_output.join("manifest.json"),
        &json!({
            "candidate_count":candidates.len(),
            "files":root_files,
            "schema_version":1,
            "search_space_sha256":search_space_sha256,
            "status":"valid"
        }),
    )?;
    fs::rename(staged_output, output).map_err(|error| {
        format!(
            "atomically publish HotpotQA tuning root '{}' from '{}': {error}",
            output.display(),
            staged_output.display()
        )
    })?;
    Ok(provisional)
}

fn hotpotqa_tuning_metrics(
    validated: &ValidatedCollection,
    run: &RunExecution,
) -> Result<Value, String> {
    let qrels = qrels_by_query(&validated.qrels);
    let evidence = validated
        .evidence
        .iter()
        .map(|row| (row.query_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let mut sums = BTreeMap::<&'static str, f64>::new();
    let mut queries = Vec::with_capacity(run.queries.len());
    for query in &run.queries {
        let ordinary = query_metrics(
            &query.projected_documents,
            qrels
                .get(query.query_id.as_str())
                .ok_or_else(|| format!("missing qrels for tuning query '{}'", query.query_id))?,
        );
        let evidence_row = evidence.get(query.query_id.as_str()).ok_or_else(|| {
            format!(
                "missing evidence judgment for tuning query '{}'",
                query.query_id
            )
        })?;
        let evidence_5 = tuning_evidence(&query.projected_documents, evidence_row, 5)?;
        let evidence_10 = tuning_evidence(&query.projected_documents, evidence_row, 10)?;
        let values = [
            ("ap", ordinary.ap),
            ("judged_at_10", ordinary.judged_at_10),
            ("judged_at_5", ordinary.judged_at_5),
            ("mrr_at_10", ordinary.mrr_at_10),
            ("ndcg_at_10", ordinary.ndcg_at_10),
            ("ndcg_at_5", ordinary.ndcg_at_5),
            ("precision_at_5", ordinary.precision_at_5),
            ("recall_at_10", ordinary.recall_at_10),
            ("recall_at_5", ordinary.recall_at_5),
            ("success_at_1", ordinary.success_at_1),
            ("supporting_document_recall_at_5", evidence_5.0),
            ("supporting_document_recall_at_10", evidence_10.0),
            ("complete_evidence_recall_at_5", evidence_5.1),
            ("complete_evidence_recall_at_10", evidence_10.1),
        ];
        let metrics = values
            .iter()
            .map(|(name, value)| {
                *sums.entry(name).or_default() += value;
                ((*name).to_owned(), json!(value))
            })
            .collect::<serde_json::Map<_, _>>();
        queries.push(json!({
            "candidate_count":query.chunk_hits.len(),
            "duplicate_collapse_count":query.duplicate_collapse_count,
            "execution_status":query.execution_status,
            "metrics":metrics,
            "query_id":query.query_id
        }));
    }
    let denominator = queries.len() as f64;
    let mut aggregate = sums
        .into_iter()
        .map(|(name, value)| (name.to_owned(), json!(value / denominator)))
        .collect::<serde_json::Map<_, _>>();
    aggregate.insert("map".to_owned(), aggregate["ap"].clone());
    Ok(json!({
        "aggregate":aggregate,
        "collection_id":validated.collection.collection_id,
        "collection_version":validated.collection.collection_version,
        "declared_population_sha256":run_population_hash(validated),
        "invalid_execution":false,
        "metric_definition_version":"graph-retrieval-v3-r2",
        "per_query":queries,
        "run_id":run.run_id,
        "schema_version":1,
        "status":"valid"
    }))
}

fn tuning_evidence(
    documents: &[ProjectedDocument],
    evidence: &super::v3_schema::EvidenceJudgment,
    cutoff: usize,
) -> Result<(f64, f64), String> {
    let documents = documents
        .iter()
        .take(cutoff)
        .map(|document| document.record_id.clone())
        .collect::<BTreeSet<_>>();
    let (matched, required) =
        super::v3_graph_retrieval_execution::best_evidence(&documents, evidence)?;
    Ok((
        matched as f64 / required as f64,
        f64::from(matched == required),
    ))
}

fn run_population_hash(validated: &ValidatedCollection) -> String {
    super::v3_population::population_hash(&validated.populations.retrieval)
}

fn tuning_candidate_order(left: &Value, right: &Value) -> std::cmp::Ordering {
    for name in [
        "complete_evidence_recall_at_10",
        "ndcg_at_10",
        "map",
        "recall_at_10",
        "mrr_at_10",
    ] {
        let order = right["aggregate"][name]
            .as_f64()
            .unwrap()
            .total_cmp(&left["aggregate"][name].as_f64().unwrap());
        if !order.is_eq() {
            return order;
        }
    }
    let limits = |value: &Value| {
        let vector = value["candidate"]["vector_candidate_limit"]
            .as_u64()
            .unwrap();
        let keyword = value["candidate"]["keyword_candidate_limit"]
            .as_u64()
            .unwrap();
        (vector + keyword, vector.max(keyword))
    };
    limits(left).cmp(&limits(right)).then_with(|| {
        canonical_json(&left["candidate"])
            .unwrap()
            .cmp(&canonical_json(&right["candidate"]).unwrap())
    })
}

fn tuning_file_inventory(root: &Path, exclude_manifest: bool) -> Result<Vec<Value>, String> {
    fn collect(root: &Path, directory: &Path, files: &mut Vec<Value>) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("read tuning directory '{}': {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("read tuning entry: {error}"))?;
            if entry
                .file_type()
                .map_err(|error| format!("inspect tuning entry: {error}"))?
                .is_dir()
            {
                collect(root, &entry.path(), files)?;
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .expect("tuning file is beneath root")
                    .to_str()
                    .ok_or_else(|| "tuning artifact path is not UTF-8".to_owned())?
                    .to_owned();
                let bytes = fs::read(entry.path()).map_err(|error| {
                    format!("read tuning artifact '{}': {error}", entry.path().display())
                })?;
                files.push(json!({"bytes":bytes.len(),"path":relative,"sha256":sha256(&bytes)}));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    if exclude_manifest {
        files.retain(|entry| entry["path"] != "manifest.json");
    }
    files.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap()
            .cmp(right["path"].as_str().unwrap())
    });
    Ok(files)
}

pub(super) fn emit_qualification(
    validated: &ValidatedCollection,
    output: &Path,
) -> Result<CompleteQualificationStatus, String> {
    emit_qualification_with_failures(validated, output, &ExecutionFailures::default())
}

pub(super) fn emit_qualification_with_failures(
    validated: &ValidatedCollection,
    output: &Path,
    failures: &ExecutionFailures,
) -> Result<CompleteQualificationStatus, String> {
    if output.exists() {
        return Err(format!(
            "complete V3 qualification root '{}' already exists; a fresh directory is required",
            output.display()
        ));
    }
    let results = execute_with_failures(validated, failures)?;
    let parent = output.parent().ok_or_else(|| {
        format!(
            "complete V3 qualification root '{}' has no parent",
            output.display()
        )
    })?;
    let staging = TemporaryDirectory::new_in(parent, ".phase-1.2c-qualification-staging")?;
    let staged_output = &staging.path;
    fs::create_dir_all(staged_output.join("runs")).map_err(|error| {
        format!(
            "failed to create complete V3 qualification root '{}': {error}",
            staged_output.display()
        )
    })?;
    fs::write(
        staged_output.join("qrels.tsv"),
        &validated.bytes["qrels.tsv"],
    )
    .map_err(|error| format!("failed to write complete V3 qrels: {error}"))?;
    write_canonical_json(
        &staged_output.join("rust-results.json"),
        &serde_json::to_value(&results)
            .map_err(|error| format!("failed to encode A-C Rust results: {error}"))?,
    )?;
    write_canonical_json(
        &staged_output.join("metrics.json"),
        &metrics_artifact(validated, &results),
    )?;
    fs::write(
        staged_output.join("timing-samples.jsonl"),
        canonical_json_line(&json!({"profile":"deterministic_quality","status":"not_measured"}))?,
    )
    .map_err(|error| format!("failed to write V3 timing marker: {error}"))?;
    for run in &results.runs {
        fs::write(
            staged_output
                .join("runs")
                .join(format!("{}.trec", run.run_id)),
            trec(run, validated.collection.evaluation_depth),
        )
        .map_err(|error| format!("failed to write A-C TREC run '{}': {error}", run.run_id))?;
    }
    let graph_results = super::v3_graph_execution::emit_graph_qualification_with_failures(
        validated,
        staged_output,
        failures,
    )?;
    let graph_retrieval_results =
        super::v3_graph_retrieval_execution::emit_graph_retrieval_qualification_with_failures(
            validated,
            staged_output,
            failures,
        )?;
    write_canonical_json(
        &staged_output.join("qualification.json"),
        &json!({
            "artifact_schema":"phase-1.2c-graph-scoped-qualification-v1",
            "collection_id":validated.collection.collection_id,
            "collection_version":validated.collection.collection_version,
            "included_run_letters":["a","b","c","d","e","f","g"],
            "partial":true,
            "publication_ready":false,
            "status":if !failures.is_empty() || results.has_invalid_execution() || graph_results.has_invalid_execution() || graph_retrieval_results.has_invalid_execution() {
                "invalid_execution"
            } else {
                "qualification_only_no_final_manifest"
            }
        }),
    )?;
    fs::rename(staged_output, output).map_err(|error| {
        format!(
            "failed to atomically finalize complete V3 qualification root '{}' from '{}': {error}",
            output.display(),
            staged_output.display()
        )
    })?;
    let phase_1_2a = if results.has_invalid_execution() {
        "invalid_execution"
    } else {
        "valid"
    };
    let phase_1_2b = if graph_results.has_invalid_execution() {
        "invalid_execution"
    } else {
        "valid"
    };
    let phase_1_2c = if graph_retrieval_results.has_invalid_execution() {
        "invalid_execution"
    } else {
        "valid"
    };
    Ok(CompleteQualificationStatus {
        phase_1_2a,
        phase_1_2b,
        phase_1_2c,
        qualification: if [phase_1_2a, phase_1_2b, phase_1_2c].contains(&"invalid_execution") {
            "invalid_execution"
        } else {
            "valid"
        },
    })
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
    let first = TemporaryDirectory::new_in(&target, "phase-1.2c-rerun-a")?;
    let second = TemporaryDirectory::new_in(&target, "phase-1.2c-rerun-b")?;
    let first_output = first.path.join("qualification");
    let second_output = second.path.join("qualification");
    emit_qualification(validated, &first_output)?;
    emit_qualification(validated, &second_output)?;
    super::v3::compare_directories_with_label(
        &first_output,
        &second_output,
        "complete V3 qualification",
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
            if run.status == "invalid_execution" {
                return invalid_aware_metrics_run(validated, run);
            }
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

fn invalid_aware_metrics_run(validated: &ValidatedCollection, run: &RunExecution) -> Value {
    let qrels = qrels_by_query(&validated.qrels);
    let metric_names = [
        "ap",
        "judged_at_10",
        "judged_at_5",
        "mrr_at_10",
        "ndcg_at_10",
        "ndcg_at_5",
        "precision_at_5",
        "recall_at_10",
        "recall_at_5",
        "success_at_1",
    ];
    let queries = run
        .queries
        .iter()
        .map(|query| {
            let metrics = if query.execution_status == "invalid_execution" {
                metric_names
                    .iter()
                    .map(|name| ((*name).to_owned(), json!({"status":"invalid_execution","value":Value::Null})))
                    .collect::<serde_json::Map<_, _>>()
            } else {
                let values = serde_json::to_value(query_metrics(
                    &query.projected_documents,
                    qrels.get(query.query_id.as_str()).unwrap(),
                ))
                .unwrap();
                values
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(name, value)| {
                        (name.clone(), json!({"status":"valid","value":value}))
                    })
                    .collect()
            };
            json!({"execution_status":query.execution_status,"metrics":metrics,"query_id":query.query_id})
        })
        .collect::<Vec<_>>();
    let macro_metrics = metric_names
        .iter()
        .map(|name| {
            let mut numerator = 0.0;
            let mut denominator = 0_u64;
            let mut invalid = 0_u64;
            for query in &queries {
                let row = &query["metrics"][*name];
                if row["status"] == "valid" {
                    numerator += row["value"].as_f64().unwrap();
                    denominator += 1;
                } else {
                    invalid += 1;
                }
            }
            (
                (*name).to_owned(),
                json!({
                    "denominator":denominator,
                    "numerator":numerator,
                    "status_counts":{
                        "excluded_pre_freeze":0,
                        "invalid_execution":invalid,
                        "not_applicable":0,
                        "undefined":0,
                        "valid":denominator
                    },
                    "value":if denominator==0 { Value::Null } else { json!(numerator/denominator as f64) }
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let invalid = run
        .queries
        .iter()
        .filter(|query| query.execution_status == "invalid_execution")
        .count();
    let identity = validated
        .runs
        .iter()
        .find(|candidate| candidate.run_id == run.run_id)
        .unwrap();
    json!({
        "counts":{
            "attempted":run.queries.len(),
            "declared":run.queries.len(),
            "excluded_pre_freeze":0,
            "invalid_execution":invalid,
            "valid_execution":run.queries.len()-invalid
        },
        "declared_population_sha256":identity.declared_hash(),
        "execution_population_sha256":identity.execution_hash(),
        "macro":macro_metrics,
        "queries":queries,
        "run_id":run.run_id,
        "status":"invalid_execution"
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
        assert!(output.join("graph-retrieval-rust-results.json").is_file());
        assert!(output
            .join("graph-retrieval-generation-fingerprints.json")
            .is_file());
        assert!(output
            .join("graph-retrieval-selection-path-equality.json")
            .is_file());
        assert!(output.join("graph-retrieval-metrics.json").is_file());
        assert!(output
            .join("graph-retrieval-paired-comparisons.json")
            .is_file());
        assert!(output
            .join("graph-retrieval-persistence-validation.json")
            .is_file());
        assert!(output.join("qualification.json").is_file());
        assert!(!output.join("manifest.json").exists());
        let marker: Value =
            serde_json::from_slice(&fs::read(output.join("qualification.json")).unwrap()).unwrap();
        assert_eq!(marker["partial"], true);
        assert_eq!(marker["publication_ready"], false);
        assert_eq!(fs::read_dir(output.join("runs")).unwrap().count(), 12);
        assert_eq!(
            marker["included_run_letters"],
            json!(["a", "b", "c", "d", "e", "f", "g"])
        );
        let graph_metrics: Value =
            serde_json::from_slice(&fs::read(output.join("graph-retrieval-metrics.json")).unwrap())
                .unwrap();
        assert_eq!(graph_metrics["runs"].as_array().unwrap().len(), 9);
        assert_eq!(
            graph_metrics["paired_comparisons"]
                .as_array()
                .unwrap()
                .len(),
            9
        );
        for run in graph_metrics["runs"].as_array().unwrap() {
            assert_eq!(run["macro"].as_object().unwrap().len(), 24);
            assert_eq!(run["micro"].as_object().unwrap().len(), 10);
            assert_eq!(
                run["counts"]["declared"].as_u64().unwrap(),
                run["queries"].as_array().unwrap().len() as u64
            );
        }
    }

    #[test]
    fn phase_1_2a_qualification_rerun_is_byte_identical() {
        let validated = validate(&fixture_root()).unwrap();
        verify_qualification_deterministic_rerun(&validated).unwrap();
    }

    #[test]
    fn serializes_canonical_invalid_execution_outcomes_without_partial_rows() {
        let validated = validate(&fixture_root()).unwrap();
        let run_id = |letter: &str, lane: Option<&str>| {
            validated
                .runs
                .iter()
                .find(|run| {
                    run.configuration["run_letter"] == letter
                        && lane.is_none_or(|lane| run.configuration["seed_lane"] == lane)
                })
                .unwrap()
                .run_id
                .clone()
        };
        let a = run_id("a", None);
        let b = run_id("b", None);
        let c = run_id("c", None);
        let d_explicit = run_id("d", Some("explicit"));
        let d_team = run_id("d", Some("team"));
        let e_explicit = run_id("e", Some("explicit"));
        let e_topic = run_id("e", Some("topic"));
        let f_explicit = run_id("f", Some("explicit"));
        let g_explicit = run_id("g", Some("explicit"));
        let mut failures = ExecutionFailures::default();
        failures.query(&a, "qa", FailureReason::ContractViolation);
        failures.run(&b, FailureReason::ContractViolation);
        failures.run(&c, FailureReason::ContractViolation);
        failures.run(&d_explicit, FailureReason::StaleSelection);
        failures.run(&d_team, FailureReason::ContractViolation);
        failures.run(&d_team, FailureReason::ReloadMismatch);
        failures.run(&d_team, FailureReason::GenerationMismatch);
        failures.run(&d_team, FailureReason::StaleSelection);
        failures.run(&e_topic, FailureReason::PersistenceMismatch);
        failures.run(&f_explicit, FailureReason::ReloadMismatch);
        failures.run(&g_explicit, FailureReason::NonDeterministicRanking);

        let first = TemporaryDirectory::new("vectorkit-v3-invalid-a").unwrap();
        let second = TemporaryDirectory::new("vectorkit-v3-invalid-b").unwrap();
        let first_output = first.path.join("qualification");
        let second_output = second.path.join("qualification");
        emit_qualification_with_failures(&validated, &first_output, &failures).unwrap();
        emit_qualification_with_failures(&validated, &second_output, &failures).unwrap();
        super::super::v3::compare_directories_with_label(
            &first_output,
            &second_output,
            "invalid qualification",
        )
        .unwrap();

        let read = |name: &str| -> Value {
            serde_json::from_slice(&fs::read(first_output.join(name)).unwrap()).unwrap()
        };
        let result_files = [
            "rust-results.json",
            "graph-rust-results.json",
            "graph-retrieval-rust-results.json",
        ];
        let mut statuses = BTreeMap::new();
        for file in result_files {
            let value = read(file);
            for run in value["runs"].as_array().unwrap() {
                statuses.insert(run["run_id"].as_str().unwrap().to_owned(), run.clone());
            }
        }
        let a_run = &statuses[&a];
        assert_eq!(a_run["status"], "invalid_execution");
        assert_eq!(
            a_run["queries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|query| query["execution_status"] == "invalid_execution")
                .count(),
            1
        );
        assert!(statuses[&e_explicit]["status"] == "valid");
        for (run_id, expected) in [
            (&b, "contract_violation"),
            (&c, "contract_violation"),
            (&d_explicit, "stale_selection"),
            (&d_team, "generation_mismatch"),
            (&e_topic, "persistence_mismatch"),
            (&f_explicit, "reload_mismatch"),
            (&g_explicit, "non_deterministic_ranking"),
        ] {
            let run = &statuses[run_id];
            assert_eq!(run["status"], "invalid_execution");
            for query in run["queries"].as_array().unwrap() {
                if query["execution_status"] == "excluded_pre_freeze" {
                    assert!(matches!(
                        query["status_reason"].as_str(),
                        Some("derived_seed_no_match" | "derived_seed_ambiguous")
                    ));
                    continue;
                }
                assert_eq!(query["execution_status"], "invalid_execution");
                assert_eq!(query["status_reason"], expected);
                assert!(query["chunk_hits"].as_array().unwrap().is_empty());
                assert!(query["projected_documents"].as_array().unwrap().is_empty());
                assert_eq!(query["duplicate_collapse_count"], 0);
            }
        }

        for (directory, extension) in [("graph-selections", "jsonl"), ("graph-paths", "jsonl")] {
            for run_id in [&d_explicit, &d_team, &e_topic, &f_explicit, &g_explicit] {
                assert!(fs::read(
                    first_output
                        .join(directory)
                        .join(format!("{run_id}.{extension}"))
                )
                .unwrap()
                .is_empty());
            }
        }
        for run_id in [&b, &c, &e_topic, &f_explicit, &g_explicit] {
            assert!(
                fs::read(first_output.join("runs").join(format!("{run_id}.trec")))
                    .unwrap()
                    .is_empty()
            );
        }
        let a_trec = String::from_utf8(
            fs::read(first_output.join("runs").join(format!("{a}.trec"))).unwrap(),
        )
        .unwrap();
        assert!(!a_trec.lines().any(|line| line.starts_with("qa ")));
        assert!(a_trec.lines().any(|line| line.starts_with("qb ")));

        for file in [
            "metrics.json",
            "graph-metrics.json",
            "graph-retrieval-metrics.json",
        ] {
            let value = read(file);
            for run in value["runs"].as_array().unwrap() {
                if statuses[run["run_id"].as_str().unwrap()]["status"] == "invalid_execution" {
                    for query in run["queries"].as_array().unwrap() {
                        if query["execution_status"] == "invalid_execution" {
                            assert!(query["metrics"]
                                .as_object()
                                .unwrap()
                                .values()
                                .all(|metric| {
                                    metric["status"] == "invalid_execution"
                                        && metric["value"].is_null()
                                }));
                        }
                    }
                }
            }
        }
        let paired = read("graph-retrieval-metrics.json");
        assert!(paired["paired_comparisons"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["status"] == "invalid_execution")
            .all(|row| row["metrics"]
                .as_object()
                .unwrap()
                .values()
                .all(|metric| metric["delta"].is_null())));
        assert_eq!(read("qualification.json")["status"], "invalid_execution");
    }

    #[test]
    fn invalid_graph_run_leaves_no_partial_qualification_artifact() {
        let mut validated = validate(&fixture_root()).unwrap();
        validated
            .runs
            .iter_mut()
            .find(|run| run.configuration["run_letter"] == "d")
            .unwrap()
            .logical_run_sha256
            .push_str("-invalid");
        let temporary = TemporaryDirectory::new("vectorkit-v3-invalid-finalization").unwrap();
        let output = temporary.path.join("qualification");

        let error = emit_qualification(&validated, &output).unwrap_err();

        assert!(error.contains("identity or population changed"));
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&temporary.path).unwrap().count(), 0);
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
