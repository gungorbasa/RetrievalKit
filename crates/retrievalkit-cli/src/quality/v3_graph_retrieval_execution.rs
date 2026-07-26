use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use retrievalkit_core::{HybridHit, HybridQuery, SearchQuery, VectorEncoding};
use retrievalkit_graph::{
    Direction, GraphPathEdge, GraphQuery, GraphRetrievalDatabase, NodeId, NodeSource, QueryLimits,
    RelationshipType, Traverse, TruncationReason,
};
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::v3::retrieval_generation_fingerprint;
use super::v3_canonical::{canonical_json, canonical_json_line, write_canonical_json};
use super::v3_execution_status::{classify_query_failure, ExecutionFailures, FailureReason};
use super::v3_graph_input::build_graph_retrieval_database;
use super::v3_ingestion::{convert_filter, ProductionQueryInput, V3ProductionInputs};
use super::v3_runs::RunIdentity;
use super::v3_schema::{EvidenceJudgment, ExpectedPaths, Qrel, Query};
use super::v3_seed::{resolve_seeds, DerivedSeedOutcome, ResolvedSeed, SeedResolutionSet};
use super::v3_validation::ValidatedCollection;

const SEMANTIC_RUNS: [(&str, &str, &str); 6] = [
    (
        "v3-e-graph-semantic-f32-explicit-cfg-d2855327ee28",
        "fd70339f21946498b010c4d26e719158212a9de0a2e745fcbc4d75b3c0ccdb25",
        "explicit",
    ),
    (
        "v3-e-graph-semantic-f32-topic-cfg-dd783bc155d4",
        "665dc02290fb825c82a55c728febd3bb8c1e98e9c7cc1fd475481aa0b9cccdd8",
        "topic",
    ),
    (
        "v3-e-graph-semantic-f32-team-cfg-9d005ed09abd",
        "ffdf1b57a1cab91c5e3ecb0f7841a3ca69f8db8f58531c1c4f943ec85a3a7a02",
        "team",
    ),
    (
        "v3-f-graph-semantic-i8-explicit-cfg-9199f34e596a",
        "1825b9e865bdd436095e5d98984a1ef9faf83dbe02ffa3268e04d463a5fd4de2",
        "explicit",
    ),
    (
        "v3-f-graph-semantic-i8-topic-cfg-748772f67f91",
        "da4bbb529aaf3ba23fa09177f62a7f760f018438d499dae00641fa2720622cd8",
        "topic",
    ),
    (
        "v3-f-graph-semantic-i8-team-cfg-c9fe28bfe8a2",
        "9e3b11888396550e38aafcec9baffdd970c588a838c561cecb3655e66b4b3f77",
        "team",
    ),
];
const HYBRID_RUNS: [(&str, &str, &str); 3] = [
    (
        "v3-g-graph-weighted-i8-explicit-cfg-f5f6dfcae573",
        "91a780087bce21816e0a71017146d19fdc87e1b0d38b3fea2a02e36254bec0aa",
        "explicit",
    ),
    (
        "v3-g-graph-weighted-i8-topic-cfg-36c6887ab88d",
        "1a6c8c0e321bd3b92194ede4257f041eaddcdf2e9e4388bbebb3ad9b006218c2",
        "topic",
    ),
    (
        "v3-g-graph-weighted-i8-team-cfg-0562c721d6e7",
        "0f0022104a1921d80f09e302e653a1877ef502d363f70a9dc46dc7c0c0bbcf7a",
        "team",
    ),
];
const METRIC_NAMES: [&str; 24] = [
    "ap",
    "candidate_complete_evidence",
    "candidate_recall",
    "candidate_reduction_ratio",
    "complete_evidence_recall_at_10",
    "complete_evidence_recall_at_5",
    "empty_scope",
    "judged_at_10",
    "judged_at_5",
    "mrr_at_10",
    "ndcg_at_10",
    "ndcg_at_5",
    "path_accuracy",
    "precision_at_5",
    "recall_at_10",
    "recall_at_5",
    "success_at_1",
    "supporting_document_recall_at_10",
    "supporting_document_recall_at_5",
    "truncated",
    "truncated_max_hops",
    "truncated_max_results",
    "truncated_max_visited",
    "truncated_max_working_bytes",
];
const PAIRED_METRICS: [&str; 14] = [
    "ap",
    "complete_evidence_recall_at_10",
    "complete_evidence_recall_at_5",
    "judged_at_10",
    "judged_at_5",
    "mrr_at_10",
    "ndcg_at_10",
    "ndcg_at_5",
    "precision_at_5",
    "recall_at_10",
    "recall_at_5",
    "success_at_1",
    "supporting_document_recall_at_10",
    "supporting_document_recall_at_5",
];
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct GraphRetrievalQualificationResults {
    collection_id: String,
    collection_version: String,
    runs: Vec<RunExecution>,
    schema_version: u8,
}

impl GraphRetrievalQualificationResults {
    pub(super) fn has_invalid_execution(&self) -> bool {
        self.runs
            .iter()
            .any(|run| run.status == "invalid_execution")
    }
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

#[derive(Debug, Clone, PartialEq)]
struct RunArtifacts {
    result: RunExecution,
    path_rows: Vec<Value>,
    projection_rows: Vec<Value>,
    selection_rows: Vec<Value>,
}

#[derive(Debug, Clone)]
pub(super) struct LockedGraphRetrievalState {
    runs: Vec<RunArtifacts>,
}

#[derive(Debug, Clone, PartialEq)]
struct ValidQuery {
    result: QueryExecution,
    path_rows: Vec<Value>,
    projection_row: Value,
    selection_row: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PersistenceValidation {
    generation_equal: bool,
    path_equal: bool,
    projection_equal: bool,
    ranking_equal: bool,
    run_id: String,
    save_validate_load_equivalent: bool,
    selection_equal: bool,
    stable_generation_fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Metric {
    status: &'static str,
    value: Option<f64>,
}

impl Metric {
    fn value(value: f64) -> Self {
        Self {
            status: "valid",
            value: Some(value),
        }
    }

    fn status(status: &'static str) -> Self {
        Self {
            status,
            value: None,
        }
    }

    fn json(self) -> Value {
        json!({"status":self.status,"value":self.value})
    }
}

pub(super) fn emit_graph_retrieval_qualification_with_failures(
    validated: &ValidatedCollection,
    output: &Path,
    failures: &ExecutionFailures,
) -> Result<GraphRetrievalQualificationResults, String> {
    validate_frozen_semantic_runs(validated)?;
    validate_frozen_hybrid_runs(validated)?;
    let seeds = resolve_seeds(validated)?;
    let inputs = V3ProductionInputs::from_validated(validated)?;
    let source_queries = validated
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut runs = Vec::new();
    let mut persistence = Vec::new();
    let mut fingerprints = BTreeMap::new();
    for run in validated.runs.iter().filter(|run| {
        matches!(
            run.configuration["run_letter"].as_str(),
            Some("e" | "f" | "g")
        )
    }) {
        let letter = run.configuration["run_letter"].as_str().unwrap();
        let encoding = match letter {
            "e" => VectorEncoding::F32,
            "f" | "g" => VectorEncoding::I8ScalarQuantized,
            _ => unreachable!(),
        };
        let (preimage, fingerprint) = retrieval_generation_fingerprint(validated, letter)?;
        fingerprints.insert(fingerprint.clone(), preimage);
        let database = build_graph_retrieval_database(validated, encoding)?;
        let (artifacts, validation) = execute_run_with_persistence_and_failures(
            validated,
            run,
            &database,
            &inputs,
            &source_queries,
            &seeds,
            &fingerprint,
            failures,
        )?;
        runs.push(artifacts);
        persistence.push(validation);
    }
    runs.sort_by(|left, right| left.result.run_id.cmp(&right.result.run_id));
    persistence.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    let expected_run_count = validated
        .runs
        .iter()
        .filter(|run| {
            matches!(
                run.configuration["run_letter"].as_str(),
                Some("e" | "f" | "g")
            )
        })
        .count();
    if runs.len() != expected_run_count {
        return Err(format!(
            "V3 Phase 1.2c expected {expected_run_count} E-G runs, actual {}",
            runs.len(),
        ));
    }

    let mut projection_rows = Vec::new();
    for run in &runs {
        write_jsonl(
            &output
                .join("graph-selections")
                .join(format!("{}.jsonl", run.result.run_id)),
            &run.selection_rows,
        )?;
        write_jsonl(
            &output
                .join("graph-paths")
                .join(format!("{}.jsonl", run.result.run_id)),
            &run.path_rows,
        )?;
        fs::write(
            output
                .join("runs")
                .join(format!("{}.trec", run.result.run_id)),
            trec(&run.result, validated.collection.evaluation_depth),
        )
        .map_err(|error| format!("write graph-scoped semantic TREC: {error}"))?;
        projection_rows.extend(run.projection_rows.clone());
    }
    projection_rows.sort_by_key(|row| {
        (
            row["run_id"].as_str().unwrap().to_owned(),
            row["query_id"].as_str().unwrap().to_owned(),
        )
    });
    write_jsonl(
        &output.join("graph-retrieval-projection-identities.jsonl"),
        &projection_rows,
    )?;
    let equality = validate_selection_path_equality_with_d(validated, output, &runs)?;
    let results = GraphRetrievalQualificationResults {
        collection_id: validated.collection.collection_id.clone(),
        collection_version: validated.collection.collection_version.clone(),
        runs: runs.iter().map(|run| run.result.clone()).collect(),
        schema_version: 3,
    };
    write_canonical_json(
        &output.join("graph-retrieval-rust-results.json"),
        &serde_json::to_value(&results)
            .map_err(|error| format!("encode graph retrieval results: {error}"))?,
    )?;
    write_canonical_json(
        &output.join("graph-retrieval-generation-fingerprints.json"),
        &json!({
            "fingerprints":fingerprints.into_iter().map(|(fingerprint,preimage)|json!({"fingerprint":fingerprint,"preimage":preimage})).collect::<Vec<_>>(),
            "schema_version":1
        }),
    )?;
    write_canonical_json(
        &output.join("graph-retrieval-selection-path-equality.json"),
        &equality,
    )?;
    let (metrics, paired) = metrics_and_paired_artifacts(validated, &runs, output)?;
    write_canonical_json(&output.join("graph-retrieval-metrics.json"), &metrics)?;
    write_canonical_json(
        &output.join("graph-retrieval-paired-comparisons.json"),
        &paired,
    )?;
    write_canonical_json(
        &output.join("graph-retrieval-persistence-validation.json"),
        &json!({"runs":persistence,"schema_version":1,"status":if runs.iter().any(|run|run.result.status=="invalid_execution") {"invalid_execution"} else {"valid"}}),
    )?;
    Ok(results)
}

pub(super) fn emit_locked_graph_retrieval_rankings(
    validated: &ValidatedCollection,
    output: &Path,
) -> Result<LockedGraphRetrievalState, String> {
    validate_frozen_semantic_runs(validated)?;
    validate_frozen_hybrid_runs(validated)?;
    let seeds = resolve_seeds(validated)?;
    let inputs = V3ProductionInputs::from_validated(validated)?;
    let source_queries = validated
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut runs = Vec::new();
    let mut persistence = Vec::new();
    let mut fingerprints = BTreeMap::new();
    for run in validated.runs.iter().filter(|run| {
        matches!(
            run.configuration["run_letter"].as_str(),
            Some("e" | "f" | "g")
        )
    }) {
        let letter = run.configuration["run_letter"].as_str().unwrap();
        let encoding = match letter {
            "e" => VectorEncoding::F32,
            "f" | "g" => VectorEncoding::I8ScalarQuantized,
            _ => unreachable!(),
        };
        let (preimage, fingerprint) = retrieval_generation_fingerprint(validated, letter)?;
        fingerprints.insert(fingerprint.clone(), preimage);
        let database = build_graph_retrieval_database(validated, encoding)?;
        let (artifacts, validation) = execute_run_with_persistence_and_failures(
            validated,
            run,
            &database,
            &inputs,
            &source_queries,
            &seeds,
            &fingerprint,
            &ExecutionFailures::default(),
        )?;
        if artifacts.result.status != "valid" {
            return Err("locked graph retrieval contains invalid_execution".to_owned());
        }
        runs.push(artifacts);
        persistence.push(validation);
    }
    runs.sort_by(|left, right| left.result.run_id.cmp(&right.result.run_id));
    persistence.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    let mut projection_rows = Vec::new();
    for run in &runs {
        write_jsonl(
            &output
                .join("graph-selections")
                .join(format!("{}.jsonl", run.result.run_id)),
            &run.selection_rows,
        )?;
        write_jsonl(
            &output
                .join("graph-paths")
                .join(format!("{}.jsonl", run.result.run_id)),
            &run.path_rows,
        )?;
        fs::write(
            output
                .join("runs")
                .join(format!("{}.trec", run.result.run_id)),
            trec(&run.result, validated.collection.evaluation_depth),
        )
        .map_err(|error| format!("write locked graph retrieval TREC: {error}"))?;
        projection_rows.extend(run.projection_rows.clone());
    }
    projection_rows.sort_by_key(|row| {
        (
            row["run_id"].as_str().unwrap().to_owned(),
            row["query_id"].as_str().unwrap().to_owned(),
        )
    });
    write_jsonl(
        &output.join("graph-retrieval-projection-identities.jsonl"),
        &projection_rows,
    )?;
    let equality = validate_selection_path_equality_with_d(validated, output, &runs)?;
    let results = GraphRetrievalQualificationResults {
        collection_id: validated.collection.collection_id.clone(),
        collection_version: validated.collection.collection_version.clone(),
        runs: runs.iter().map(|run| run.result.clone()).collect(),
        schema_version: 3,
    };
    write_canonical_json(
        &output.join("graph-retrieval-rust-results.json"),
        &serde_json::to_value(&results)
            .map_err(|error| format!("encode locked graph retrieval results: {error}"))?,
    )?;
    write_canonical_json(
        &output.join("graph-retrieval-generation-fingerprints.json"),
        &json!({
            "fingerprints":fingerprints.into_iter().map(|(fingerprint,preimage)|json!({"fingerprint":fingerprint,"preimage":preimage})).collect::<Vec<_>>(),
            "schema_version":1
        }),
    )?;
    write_canonical_json(
        &output.join("graph-retrieval-selection-path-equality.json"),
        &equality,
    )?;
    write_canonical_json(
        &output.join("graph-retrieval-persistence-validation.json"),
        &json!({"runs":persistence,"schema_version":1,"status":"valid"}),
    )?;
    Ok(LockedGraphRetrievalState { runs })
}

pub(super) fn score_locked_graph_retrieval_rankings(
    validated: &ValidatedCollection,
    state: &LockedGraphRetrievalState,
    output: &Path,
) -> Result<(), String> {
    let (metrics, paired) = metrics_and_paired_artifacts(validated, &state.runs, output)?;
    write_canonical_json(&output.join("graph-retrieval-metrics.json"), &metrics)?;
    write_canonical_json(
        &output.join("graph-retrieval-paired-comparisons.json"),
        &paired,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn execute_run_with_persistence(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    database: &GraphRetrievalDatabase,
    inputs: &V3ProductionInputs,
    source_queries: &BTreeMap<&str, &Query>,
    seeds: &SeedResolutionSet,
    fingerprint: &str,
) -> Result<(RunArtifacts, PersistenceValidation), String> {
    execute_run_with_persistence_and_failures(
        validated,
        run,
        database,
        inputs,
        source_queries,
        seeds,
        fingerprint,
        &ExecutionFailures::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_run_with_persistence_and_failures(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    database: &GraphRetrievalDatabase,
    inputs: &V3ProductionInputs,
    source_queries: &BTreeMap<&str, &Query>,
    seeds: &SeedResolutionSet,
    fingerprint: &str,
    injected_failures: &ExecutionFailures,
) -> Result<(RunArtifacts, PersistenceValidation), String> {
    let mut before = execute_run(
        validated,
        run,
        database,
        inputs,
        source_queries,
        seeds,
        fingerprint,
    )?;
    let repeated = execute_run(
        validated,
        run,
        database,
        inputs,
        source_queries,
        seeds,
        fingerprint,
    )?;
    let mut failures = injected_failures.clone();
    if before != repeated {
        failures.run(run.run_id.clone(), FailureReason::NonDeterministicRanking);
    }
    let temporary = TemporaryDirectory::new("retrievalkit-v3-phase-1-2c-persistence")?;
    let persisted = temporary.path.join("database");
    database
        .save_to_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2c save '{}': {error}", run.run_id))?;
    GraphRetrievalDatabase::validate_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2c validate '{}': {error}", run.run_id))?;
    let loaded = GraphRetrievalDatabase::load_from_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2c reload '{}': {error}", run.run_id))?;
    if verify_persisted_database(database, &loaded, run).is_err() {
        failures.run(run.run_id.clone(), FailureReason::ReloadMismatch);
    }
    let after = execute_run(
        validated,
        run,
        &loaded,
        inputs,
        source_queries,
        seeds,
        fingerprint,
    )?;
    if before != after {
        failures.run(run.run_id.clone(), FailureReason::PersistenceMismatch);
    }
    apply_failures(run, &mut before, &failures);
    let run_reason = failures.run_reason(&run.run_id);
    let persistence_equivalent = !matches!(
        run_reason,
        Some(FailureReason::PersistenceMismatch | FailureReason::ReloadMismatch)
    );
    Ok((
        before,
        PersistenceValidation {
            generation_equal: !matches!(
                run_reason,
                Some(FailureReason::GenerationMismatch | FailureReason::ReloadMismatch)
            ),
            path_equal: !matches!(run_reason, Some(FailureReason::PersistenceMismatch)),
            projection_equal: !matches!(run_reason, Some(FailureReason::PersistenceMismatch)),
            ranking_equal: !matches!(
                run_reason,
                Some(FailureReason::PersistenceMismatch | FailureReason::NonDeterministicRanking)
            ),
            run_id: run.run_id.clone(),
            save_validate_load_equivalent: persistence_equivalent,
            selection_equal: !matches!(
                run_reason,
                Some(FailureReason::StaleSelection | FailureReason::PersistenceMismatch)
            ),
            stable_generation_fingerprint: fingerprint.to_owned(),
        },
    ))
}

fn verify_persisted_database(
    before: &GraphRetrievalDatabase,
    after: &GraphRetrievalDatabase,
    run: &RunIdentity,
) -> Result<(), String> {
    let stable_identities = |database: &GraphRetrievalDatabase| {
        database
            .corpus()
            .chunk_identities()
            .map(|(identity, chunk_id)| {
                (
                    identity.record_id.as_str().to_owned(),
                    identity.chunk_key.as_str().to_owned(),
                    chunk_id,
                )
            })
            .collect::<Vec<_>>()
    };
    if before.corpus().corpus_id() != after.corpus().corpus_id()
        || before.corpus().generation() != after.corpus().generation()
        || stable_identities(before) != stable_identities(after)
        || before.retrieval().retrieval().vector_encoding()
            != after.retrieval().retrieval().vector_encoding()
        || before.retrieval().retrieval().dimension() != after.retrieval().retrieval().dimension()
        || before.retrieval().retrieval().metric() != after.retrieval().retrieval().metric()
        || before.retrieval().retrieval().has_bm25() != after.retrieval().retrieval().has_bm25()
        || before.graph().schema() != after.graph().schema()
        || before.graph().build_stats() != after.graph().build_stats()
        || before.graph().node_count() != after.graph().node_count()
        || before.graph().edge_count() != after.graph().edge_count()
    {
        return Err(format!(
            "V3 Phase 1.2c reload_mismatch for run '{}'",
            run.run_id
        ));
    }
    Ok(())
}

fn metrics_and_paired_artifacts(
    validated: &ValidatedCollection,
    runs: &[RunArtifacts],
    output: &Path,
) -> Result<(Value, Value), String> {
    let mut run_metrics = Vec::new();
    for artifacts in runs {
        let identity = validated
            .runs
            .iter()
            .find(|run| run.run_id == artifacts.result.run_id)
            .ok_or_else(|| format!("missing run identity '{}'", artifacts.result.run_id))?;
        run_metrics.push(metrics_for_run(validated, identity, artifacts)?);
    }
    run_metrics.sort_by_key(|run| run["run_id"].as_str().unwrap().to_owned());
    let baseline: Value = serde_json::from_slice(
        &fs::read(output.join("rust-results.json"))
            .map_err(|error| format!("read finalized A-C results: {error}"))?,
    )
    .map_err(|error| format!("parse finalized A-C results: {error}"))?;
    let (paired_contract, paired_diagnostics) =
        paired_comparisons(validated, &baseline, &run_metrics, runs)?;
    let invalid = baseline["runs"]
        .as_array()
        .is_some_and(|runs| runs.iter().any(|run| run["status"] == "invalid_execution"))
        || run_metrics
            .iter()
            .any(|run| run["status"] == "invalid_execution");
    Ok((
        json!({
            "collection_id":validated.collection.collection_id,
            "collection_version":validated.collection.collection_version,
            "metric_definition_version":"graph-retrieval-v3-r2",
            "paired_comparisons":paired_contract,
            "partial":true,
            "publication_ready":false,
            "runs":run_metrics,
            "schema_version":3
        }),
        json!({
            "comparisons":paired_diagnostics,
            "schema_version":1,
            "status":if invalid {"invalid_execution"} else {"valid"}
        }),
    ))
}

fn metrics_for_run(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    artifacts: &RunArtifacts,
) -> Result<Value, String> {
    let lane = run.configuration["seed_lane"]
        .as_str()
        .ok_or_else(|| format!("run '{}' has no seed lane", run.run_id))?;
    let queries_by_id = validated
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let result_by_id = artifacts
        .result
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let selections = artifacts
        .selection_rows
        .iter()
        .map(|row| (row["query_id"].as_str().unwrap(), row))
        .collect::<BTreeMap<_, _>>();
    let projections = artifacts
        .projection_rows
        .iter()
        .map(|row| (row["query_id"].as_str().unwrap(), row))
        .collect::<BTreeMap<_, _>>();
    let mut query_rows = Vec::new();
    for query_id in &run.declared {
        let result = result_by_id[query_id.as_str()];
        if result.execution_status == "excluded_pre_freeze" {
            let metrics = METRIC_NAMES
                .iter()
                .map(|name| {
                    (
                        (*name).to_owned(),
                        Metric::status("excluded_pre_freeze").json(),
                    )
                })
                .collect::<Map<_, _>>();
            query_rows.push(json!({
                "candidate_counts":Value::Null,
                "execution_status":"excluded_pre_freeze",
                "metrics":metrics,
                "query_id":query_id
            }));
            continue;
        }
        if result.execution_status == "invalid_execution" {
            let metrics = METRIC_NAMES
                .iter()
                .map(|name| {
                    (
                        (*name).to_owned(),
                        Metric::status("invalid_execution").json(),
                    )
                })
                .collect::<Map<_, _>>();
            query_rows.push(json!({
                "candidate_counts":Value::Null,
                "execution_status":"invalid_execution",
                "metrics":metrics,
                "query_id":query_id
            }));
            continue;
        }
        let query = queries_by_id[query_id.as_str()];
        let selection = selections[query_id.as_str()];
        let projection = projections[query_id.as_str()];
        let path_rows = artifacts
            .path_rows
            .iter()
            .filter(|row| row["query_id"] == *query_id)
            .collect::<Vec<_>>();
        let metrics = query_metric_values(
            validated, lane, query, result, selection, projection, &path_rows,
        )?;
        query_rows.push(json!({
            "candidate_counts":{
                "eligible_chunks":selection["eligible_corpus_chunks_after_filter"],
                "projected_chunks":selection["projected_chunks_after_filter"]
            },
            "execution_status":"valid",
            "metrics":metric_map_json(&metrics),
            "query_id":query_id
        }));
    }
    let macro_metrics = macro_metrics(&query_rows, &METRIC_NAMES)?;
    let micro = micro_metrics(validated, &query_rows, artifacts)?;
    Ok(json!({
        "counts":{
            "attempted":run.execution.len(),
            "declared":run.declared.len(),
            "excluded_pre_freeze":run.declared.len()-run.execution.len(),
            "invalid_execution":query_rows.iter().filter(|row|row["execution_status"]=="invalid_execution").count(),
            "valid_execution":query_rows.iter().filter(|row|row["execution_status"]=="valid").count()
        },
        "declared_population_sha256":run.declared_hash(),
        "execution_population_sha256":run.execution_hash(),
        "macro":macro_metrics,
        "micro":micro,
        "queries":query_rows,
        "run_id":run.run_id,
        "status":if artifacts.result.status=="valid" {"valid"} else {"invalid_execution"}
    }))
}

fn query_metric_values(
    validated: &ValidatedCollection,
    lane: &str,
    query: &Query,
    result: &QueryExecution,
    selection: &Value,
    projection: &Value,
    path_rows: &[&Value],
) -> Result<BTreeMap<&'static str, Metric>, String> {
    let mut metrics = METRIC_NAMES
        .iter()
        .map(|name| (*name, Metric::status("not_applicable")))
        .collect::<BTreeMap<_, _>>();
    let documents = result
        .projected_documents
        .iter()
        .map(|document| document.record_id.clone())
        .collect::<Vec<_>>();
    let qrels = qrels_for_query(&validated.qrels, &query.query_id);
    insert_retrieval_metrics(&mut metrics, &documents, &qrels);
    let projected_chunks = selection["projected_chunks_after_filter"]
        .as_u64()
        .ok_or_else(|| "selection projected chunk count missing".to_owned())?
        as usize;
    let eligible_chunks = selection["eligible_corpus_chunks_after_filter"]
        .as_u64()
        .ok_or_else(|| "selection eligible chunk count missing".to_owned())?
        as usize;
    metrics.insert(
        "candidate_reduction_ratio",
        if projected_chunks == 0 {
            Metric::status("undefined")
        } else {
            Metric::value(eligible_chunks as f64 / projected_chunks as f64)
        },
    );
    metrics.insert(
        "empty_scope",
        Metric::value(if projected_chunks == 0 { 1.0 } else { 0.0 }),
    );
    let reason = selection["truncated_reason"].as_str();
    metrics.insert(
        "truncated",
        Metric::value(if reason.is_some() { 1.0 } else { 0.0 }),
    );
    for (name, expected) in [
        ("truncated_max_hops", "max_hops"),
        ("truncated_max_results", "max_results"),
        ("truncated_max_visited", "max_visited"),
        ("truncated_max_working_bytes", "max_working_bytes"),
    ] {
        metrics.insert(
            name,
            Metric::value(if reason == Some(expected) { 1.0 } else { 0.0 }),
        );
    }
    let candidate_documents = projection["candidates"]
        .as_array()
        .ok_or_else(|| "projection candidates missing".to_owned())?
        .iter()
        .map(|candidate| candidate["record_id"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    if query.tasks.iter().any(|task| task == "evidence") {
        let evidence = evidence_for_query(validated, &query.query_id)?;
        let (candidate_matched, candidate_required) =
            best_evidence(&candidate_documents, evidence)?;
        metrics.insert(
            "candidate_recall",
            Metric::value(candidate_matched as f64 / candidate_required as f64),
        );
        metrics.insert(
            "candidate_complete_evidence",
            Metric::value(if candidate_matched == candidate_required {
                1.0
            } else {
                0.0
            }),
        );
        for (cutoff, supporting_name, complete_name) in [
            (
                5,
                "supporting_document_recall_at_5",
                "complete_evidence_recall_at_5",
            ),
            (
                10,
                "supporting_document_recall_at_10",
                "complete_evidence_recall_at_10",
            ),
        ] {
            let returned = documents
                .iter()
                .take(cutoff)
                .cloned()
                .collect::<BTreeSet<_>>();
            let (matched, required) = best_evidence(&returned, evidence)?;
            metrics.insert(
                supporting_name,
                Metric::value(matched as f64 / required as f64),
            );
            metrics.insert(
                complete_name,
                Metric::value(if matched == required { 1.0 } else { 0.0 }),
            );
        }
    }
    if query.tasks.iter().any(|task| task == "path") {
        if let Some(expected) = validated
            .expected_paths
            .iter()
            .find(|row| row.query_id == query.query_id && row.seed_policy == lane)
        {
            metrics.insert(
                "path_accuracy",
                Metric::value(if path_matches(path_rows, expected)? {
                    1.0
                } else {
                    0.0
                }),
            );
        }
    }
    Ok(metrics)
}

fn insert_retrieval_metrics(
    metrics: &mut BTreeMap<&'static str, Metric>,
    documents: &[String],
    qrels: &BTreeMap<String, u8>,
) {
    metrics.insert("ap", Metric::value(average_precision(documents, qrels)));
    metrics.insert("judged_at_10", Metric::value(judged(documents, qrels, 10)));
    metrics.insert("judged_at_5", Metric::value(judged(documents, qrels, 5)));
    metrics.insert(
        "mrr_at_10",
        Metric::value(reciprocal_rank(documents, qrels, 10)),
    );
    metrics.insert("ndcg_at_10", Metric::value(ndcg(documents, qrels, 10)));
    metrics.insert("ndcg_at_5", Metric::value(ndcg(documents, qrels, 5)));
    metrics.insert(
        "precision_at_5",
        Metric::value(relevant_count(documents, qrels, 5) as f64 / 5.0),
    );
    metrics.insert("recall_at_10", Metric::value(recall(documents, qrels, 10)));
    metrics.insert("recall_at_5", Metric::value(recall(documents, qrels, 5)));
    metrics.insert(
        "success_at_1",
        Metric::value(f64::from(relevant_count(documents, qrels, 1) > 0)),
    );
}

fn metric_map_json(metrics: &BTreeMap<&'static str, Metric>) -> Value {
    Value::Object(
        metrics
            .iter()
            .map(|(name, metric)| ((*name).to_owned(), metric.json()))
            .collect(),
    )
}

fn macro_metrics(queries: &[Value], names: &[&str]) -> Result<Value, String> {
    let mut output = Map::new();
    for name in names {
        let rows = queries
            .iter()
            .map(|query| &query["metrics"][*name])
            .collect::<Vec<_>>();
        output.insert((*name).to_owned(), macro_metric(&rows)?);
    }
    Ok(Value::Object(output))
}

fn macro_metric(rows: &[&Value]) -> Result<Value, String> {
    let mut numerator = 0.0;
    let mut denominator = 0_u64;
    let mut counts = BTreeMap::from([
        ("excluded_pre_freeze", 0_u64),
        ("invalid_execution", 0),
        ("not_applicable", 0),
        ("undefined", 0),
        ("valid", 0),
    ]);
    for row in rows {
        let status = row["status"]
            .as_str()
            .ok_or_else(|| "metric status missing".to_owned())?;
        *counts
            .get_mut(status)
            .ok_or_else(|| format!("invalid metric status '{status}'"))? += 1;
        if status == "valid" {
            numerator += row["value"]
                .as_f64()
                .ok_or_else(|| "valid metric value missing".to_owned())?;
            denominator += 1;
        }
    }
    Ok(json!({
        "denominator":denominator,
        "numerator":numerator,
        "status_counts":counts,
        "value":if denominator==0{Value::Null}else{json!(numerator/denominator as f64)}
    }))
}

fn micro_metrics(
    validated: &ValidatedCollection,
    query_rows: &[Value],
    artifacts: &RunArtifacts,
) -> Result<Value, String> {
    let mut supporting_5 = (0_usize, 0_usize);
    let mut supporting_10 = (0_usize, 0_usize);
    let mut candidate = (0_usize, 0_usize);
    let mut eligible = 0_usize;
    let mut projected = 0_usize;
    let mut empty = 0_usize;
    let mut truncated = BTreeMap::from([
        ("all", 0_usize),
        ("max_hops", 0),
        ("max_results", 0),
        ("max_visited", 0),
        ("max_working_bytes", 0),
    ]);
    for row in query_rows
        .iter()
        .filter(|row| row["execution_status"] == "valid")
    {
        let query_id = row["query_id"].as_str().unwrap();
        let counts = &row["candidate_counts"];
        let eligible_count = counts["eligible_chunks"].as_u64().unwrap() as usize;
        let projected_count = counts["projected_chunks"].as_u64().unwrap() as usize;
        eligible += eligible_count;
        projected += projected_count;
        empty += usize::from(projected_count == 0);
        let selection = artifacts
            .selection_rows
            .iter()
            .find(|selection| selection["query_id"] == query_id)
            .unwrap();
        if let Some(reason) = selection["truncated_reason"].as_str() {
            *truncated.get_mut("all").unwrap() += 1;
            *truncated.get_mut(reason).unwrap() += 1;
        }
        let query = validated
            .queries
            .iter()
            .find(|query| query.query_id == query_id)
            .unwrap();
        if query.tasks.iter().any(|task| task == "evidence") {
            let evidence = evidence_for_query(validated, query_id)?;
            let result = artifacts
                .result
                .queries
                .iter()
                .find(|result| result.query_id == query_id)
                .unwrap();
            let documents = result
                .projected_documents
                .iter()
                .map(|document| document.record_id.clone())
                .collect::<Vec<_>>();
            let at_5 = documents.iter().take(5).cloned().collect::<BTreeSet<_>>();
            let at_10 = documents.iter().take(10).cloned().collect::<BTreeSet<_>>();
            let projection = artifacts
                .projection_rows
                .iter()
                .find(|projection| projection["query_id"] == query_id)
                .unwrap();
            let candidates = projection["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|identity| identity["record_id"].as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>();
            let choice_5 = best_evidence(&at_5, evidence)?;
            let choice_10 = best_evidence(&at_10, evidence)?;
            let choice_candidate = best_evidence(&candidates, evidence)?;
            supporting_5.0 += choice_5.0;
            supporting_5.1 += choice_5.1;
            supporting_10.0 += choice_10.0;
            supporting_10.1 += choice_10.1;
            candidate.0 += choice_candidate.0;
            candidate.1 += choice_candidate.1;
        }
    }
    let graph_valid = query_rows
        .iter()
        .filter(|row| row["execution_status"] == "valid")
        .count();
    let ratio = |numerator: usize, denominator: usize| {
        if denominator == 0 {
            Value::Null
        } else {
            json!(numerator as f64 / denominator as f64)
        }
    };
    Ok(json!({
        "candidate_recall":{"matched_documents":candidate.0,"required_documents":candidate.1,"value":ratio(candidate.0,candidate.1)},
        "candidate_reduction_ratio":{"candidate_chunks":projected,"eligible_chunks":eligible,"value":ratio(eligible,projected)},
        "empty_scope_rate":{"empty_scopes":empty,"graph_valid_queries":graph_valid,"value":ratio(empty,graph_valid)},
        "supporting_document_recall_at_10":{"matched_documents":supporting_10.0,"required_documents":supporting_10.1,"value":ratio(supporting_10.0,supporting_10.1)},
        "supporting_document_recall_at_5":{"matched_documents":supporting_5.0,"required_documents":supporting_5.1,"value":ratio(supporting_5.0,supporting_5.1)},
        "truncation_rate":{"affected_queries":truncated["all"],"graph_valid_queries":graph_valid,"value":ratio(truncated["all"],graph_valid)},
        "truncation_rate_max_hops":{"affected_queries":truncated["max_hops"],"graph_valid_queries":graph_valid,"value":ratio(truncated["max_hops"],graph_valid)},
        "truncation_rate_max_results":{"affected_queries":truncated["max_results"],"graph_valid_queries":graph_valid,"value":ratio(truncated["max_results"],graph_valid)},
        "truncation_rate_max_visited":{"affected_queries":truncated["max_visited"],"graph_valid_queries":graph_valid,"value":ratio(truncated["max_visited"],graph_valid)},
        "truncation_rate_max_working_bytes":{"affected_queries":truncated["max_working_bytes"],"graph_valid_queries":graph_valid,"value":ratio(truncated["max_working_bytes"],graph_valid)}
    }))
}

fn paired_comparisons(
    validated: &ValidatedCollection,
    baseline_results: &Value,
    run_metrics: &[Value],
    artifacts: &[RunArtifacts],
) -> Result<(Vec<Value>, Vec<Value>), String> {
    let baseline_run_id = |letter: &str| -> Result<&str, String> {
        let baseline_letter = match letter {
            "e" => "a",
            "f" => "b",
            "g" => "c",
            actual => return Err(format!("unsupported graph retrieval run letter '{actual}'")),
        };
        validated
            .runs
            .iter()
            .find(|run| run.configuration["run_letter"] == baseline_letter)
            .map(|run| run.run_id.as_str())
            .ok_or_else(|| format!("missing baseline run letter '{baseline_letter}'"))
    };
    let mut contract_rows = Vec::new();
    let mut diagnostic_rows = Vec::new();
    for scoped in run_metrics {
        let scoped_run_id = scoped["run_id"].as_str().unwrap();
        let scoped_identity = validated
            .runs
            .iter()
            .find(|run| run.run_id == scoped_run_id)
            .unwrap();
        let letter = scoped_identity.configuration["run_letter"]
            .as_str()
            .unwrap();
        let baseline_run_id = baseline_run_id(letter)?;
        let baseline_run = baseline_results["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|run| run["run_id"] == baseline_run_id)
            .ok_or_else(|| format!("missing finalized baseline '{baseline_run_id}'"))?;
        let scoped_artifacts = artifacts
            .iter()
            .find(|run| run.result.run_id == scoped_run_id)
            .unwrap();
        let comparison_invalid = baseline_run["status"] == "invalid_execution"
            || scoped["status"] == "invalid_execution";
        let mut contract_metrics = Map::new();
        let mut diagnostic_metrics = Map::new();
        for name in PAIRED_METRICS {
            let mut baseline_rows = Vec::new();
            let mut scoped_rows = Vec::new();
            let mut wins = 0_usize;
            let mut ties = 0_usize;
            let mut losses = 0_usize;
            for query_id in &scoped_identity.execution {
                let query = validated
                    .queries
                    .iter()
                    .find(|query| query.query_id == *query_id)
                    .unwrap();
                let qrels = qrels_for_query(&validated.qrels, query_id);
                let baseline_query = baseline_run["queries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["query_id"] == *query_id)
                    .unwrap();
                let baseline_documents = baseline_query["projected_documents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|document| document["record_id"].as_str().unwrap().to_owned())
                    .collect::<Vec<_>>();
                let baseline_metric = if baseline_query["execution_status"] == "invalid_execution" {
                    Metric::status("invalid_execution").json()
                } else {
                    paired_query_metric(validated, query, name, &baseline_documents, &qrels)?
                };
                let scoped_row = scoped["queries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["query_id"] == *query_id)
                    .unwrap();
                let scoped_metric = scoped_row["metrics"][name].clone();
                if baseline_metric["status"] == "valid" && scoped_metric["status"] == "valid" {
                    let baseline_value = baseline_metric["value"].as_f64().unwrap();
                    let scoped_value = scoped_metric["value"].as_f64().unwrap();
                    match scoped_value.total_cmp(&baseline_value) {
                        std::cmp::Ordering::Greater => wins += 1,
                        std::cmp::Ordering::Equal => ties += 1,
                        std::cmp::Ordering::Less => losses += 1,
                    }
                }
                baseline_rows.push(baseline_metric);
                scoped_rows.push(scoped_metric);
            }
            let baseline_refs = baseline_rows.iter().collect::<Vec<_>>();
            let scoped_refs = scoped_rows.iter().collect::<Vec<_>>();
            let baseline_macro = macro_metric(&baseline_refs)?;
            let scoped_macro = macro_metric(&scoped_refs)?;
            let delta = if comparison_invalid {
                Value::Null
            } else {
                paired_delta(&baseline_macro, &scoped_macro)
            };
            contract_metrics.insert(
                name.to_owned(),
                json!({"baseline":baseline_macro,"delta":delta,"scoped":scoped_macro}),
            );
            let relative = if comparison_invalid {
                Value::Null
            } else {
                match (
                    baseline_macro["value"].as_f64(),
                    scoped_macro["value"].as_f64(),
                ) {
                    (Some(baseline), Some(scoped)) if baseline != 0.0 => {
                        json!((scoped - baseline) / baseline)
                    }
                    _ => Value::Null,
                }
            };
            diagnostic_metrics.insert(
                name.to_owned(),
                json!({
                    "baseline":baseline_macro["value"],
                    "delta":delta,
                    "losses":losses,
                    "relative_delta":relative,
                    "scoped":scoped_macro["value"],
                    "ties":ties,
                    "wins":wins
                }),
            );
        }
        let candidate = paired_candidate_summary(
            validated,
            baseline_run,
            scoped_artifacts,
            &scoped_identity.execution,
        )?;
        let comparison_status = if comparison_invalid {
            "invalid_execution"
        } else {
            "valid"
        };
        contract_rows.push(json!({
            "baseline_run_id":baseline_run_id,
            "metrics":contract_metrics,
            "query_population_sha256":scoped_identity.execution_hash(),
            "scoped_run_id":scoped_run_id,
            "seed_lane":scoped_identity.configuration["seed_lane"],
            "status":comparison_status
        }));
        let mut diagnostic = json!({
            "baseline_run_id":baseline_run_id,
            "candidate_and_loss":candidate,
            "metrics":diagnostic_metrics,
            "query_population_sha256":scoped_identity.execution_hash(),
            "scoped_run_id":scoped_run_id,
            "seed_lane":scoped_identity.configuration["seed_lane"]
        });
        if comparison_status == "invalid_execution" {
            diagnostic["status"] = json!(comparison_status);
        }
        diagnostic_rows.push(diagnostic);
    }
    contract_rows.sort_by_key(|row| {
        (
            row["scoped_run_id"].as_str().unwrap().to_owned(),
            row["baseline_run_id"].as_str().unwrap().to_owned(),
        )
    });
    diagnostic_rows.sort_by_key(|row| row["scoped_run_id"].as_str().unwrap().to_owned());
    Ok((contract_rows, diagnostic_rows))
}

fn paired_query_metric(
    validated: &ValidatedCollection,
    query: &Query,
    name: &str,
    documents: &[String],
    qrels: &BTreeMap<String, u8>,
) -> Result<Value, String> {
    let value = match name {
        "ap" => Some(average_precision(documents, qrels)),
        "judged_at_10" => Some(judged(documents, qrels, 10)),
        "judged_at_5" => Some(judged(documents, qrels, 5)),
        "mrr_at_10" => Some(reciprocal_rank(documents, qrels, 10)),
        "ndcg_at_10" => Some(ndcg(documents, qrels, 10)),
        "ndcg_at_5" => Some(ndcg(documents, qrels, 5)),
        "precision_at_5" => Some(relevant_count(documents, qrels, 5) as f64 / 5.0),
        "recall_at_10" => Some(recall(documents, qrels, 10)),
        "recall_at_5" => Some(recall(documents, qrels, 5)),
        "success_at_1" => Some(f64::from(relevant_count(documents, qrels, 1) > 0)),
        "supporting_document_recall_at_5"
        | "supporting_document_recall_at_10"
        | "complete_evidence_recall_at_5"
        | "complete_evidence_recall_at_10" => {
            if !query.tasks.iter().any(|task| task == "evidence") {
                return Ok(Metric::status("not_applicable").json());
            }
            let cutoff = if name.ends_with("_at_5") { 5 } else { 10 };
            let returned = documents
                .iter()
                .take(cutoff)
                .cloned()
                .collect::<BTreeSet<_>>();
            let (matched, required) =
                best_evidence(&returned, evidence_for_query(validated, &query.query_id)?)?;
            if name.starts_with("complete") {
                Some(if matched == required { 1.0 } else { 0.0 })
            } else {
                Some(matched as f64 / required as f64)
            }
        }
        actual => return Err(format!("unsupported paired metric '{actual}'")),
    };
    Ok(Metric::value(value.unwrap()).json())
}

fn paired_delta(baseline: &Value, scoped: &Value) -> Value {
    match (baseline["value"].as_f64(), scoped["value"].as_f64()) {
        (Some(baseline), Some(scoped)) => json!(scoped - baseline),
        _ => Value::Null,
    }
}

fn paired_candidate_summary(
    validated: &ValidatedCollection,
    baseline_run: &Value,
    scoped: &RunArtifacts,
    population: &BTreeSet<String>,
) -> Result<Value, String> {
    let mut eligible = 0_usize;
    let mut projected = 0_usize;
    let mut relevant_lost = 0_usize;
    let mut evidence_lost = 0_usize;
    let mut per_query = Vec::new();
    for query_id in population {
        let baseline_query = baseline_run["queries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["query_id"] == *query_id)
            .unwrap();
        let scoped_query = scoped
            .result
            .queries
            .iter()
            .find(|row| row.query_id == *query_id)
            .unwrap();
        if baseline_query["execution_status"] == "invalid_execution"
            || scoped_query.execution_status == "invalid_execution"
        {
            continue;
        }
        let selection = scoped
            .selection_rows
            .iter()
            .find(|row| row["query_id"] == *query_id)
            .unwrap();
        let eligible_count = selection["eligible_corpus_chunks_after_filter"]
            .as_u64()
            .unwrap() as usize;
        let projected_count = selection["projected_chunks_after_filter"].as_u64().unwrap() as usize;
        eligible += eligible_count;
        projected += projected_count;
        let qrels = qrels_for_query(&validated.qrels, query_id);
        let baseline_documents = baseline_query["projected_documents"]
            .as_array()
            .unwrap()
            .iter()
            .take(10)
            .map(|document| document["record_id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let scoped_documents = scoped_query
            .projected_documents
            .iter()
            .take(10)
            .map(|document| document.record_id.clone())
            .collect::<Vec<_>>();
        let baseline_relevant = relevant_count(&baseline_documents, &qrels, 10);
        let scoped_relevant = relevant_count(&scoped_documents, &qrels, 10);
        let query_relevant_lost = baseline_relevant.saturating_sub(scoped_relevant);
        relevant_lost += query_relevant_lost;
        let query = validated
            .queries
            .iter()
            .find(|query| query.query_id == *query_id)
            .unwrap();
        let query_evidence_lost = if query.tasks.iter().any(|task| task == "evidence") {
            let evidence = evidence_for_query(validated, query_id)?;
            let baseline_set = baseline_documents.into_iter().collect::<BTreeSet<_>>();
            let scoped_set = scoped_documents.into_iter().collect::<BTreeSet<_>>();
            let baseline_match = best_evidence(&baseline_set, evidence)?.0;
            let scoped_match = best_evidence(&scoped_set, evidence)?.0;
            baseline_match.saturating_sub(scoped_match)
        } else {
            0
        };
        evidence_lost += query_evidence_lost;
        per_query.push(json!({
            "eligible_chunks":eligible_count,
            "evidence_documents_lost_at_10":query_evidence_lost,
            "projected_chunks":projected_count,
            "query_id":query_id,
            "relevant_documents_lost_at_10":query_relevant_lost
        }));
    }
    Ok(json!({
        "candidate_reduction_ratio":if projected==0{Value::Null}else{json!(eligible as f64/projected as f64)},
        "eligible_chunks":eligible,
        "evidence_documents_lost_at_10":evidence_lost,
        "per_query":per_query,
        "projected_chunks":projected,
        "relevant_documents_lost_at_10":relevant_lost
    }))
}

fn qrels_for_query(qrels: &[Qrel], query_id: &str) -> BTreeMap<String, u8> {
    qrels
        .iter()
        .filter(|qrel| qrel.query_id == query_id)
        .map(|qrel| (qrel.record_id.clone(), qrel.relevance))
        .collect()
}

fn evidence_for_query<'a>(
    validated: &'a ValidatedCollection,
    query_id: &str,
) -> Result<&'a EvidenceJudgment, String> {
    validated
        .evidence
        .iter()
        .find(|row| row.query_id == query_id)
        .ok_or_else(|| format!("missing evidence judgment for '{query_id}'"))
}

pub(super) fn best_evidence(
    documents: &BTreeSet<String>,
    evidence: &EvidenceJudgment,
) -> Result<(usize, usize), String> {
    let mut choices = evidence
        .evidence_sets
        .iter()
        .map(|required| {
            let matched = required
                .iter()
                .filter(|record_id| documents.contains(*record_id))
                .count();
            Ok((matched, required.len(), canonical_json(&json!(required))?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    choices.sort_by(|left, right| {
        (right.0 * left.1)
            .cmp(&(left.0 * right.1))
            .then_with(|| right.0.cmp(&left.0))
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    choices
        .first()
        .map(|choice| (choice.0, choice.1))
        .ok_or_else(|| "validated evidence alternatives are empty".to_owned())
}

fn path_matches(actual_rows: &[&Value], expected: &ExpectedPaths) -> Result<bool, String> {
    let actual = actual_rows
        .iter()
        .map(|row| canonical_json(&row["edges"]))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let expected = expected
        .expected_paths
        .iter()
        .map(|path| {
            serde_json::to_value(path)
                .map_err(|error| format!("encode expected path: {error}"))
                .and_then(|value| canonical_json(&value))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(!actual.is_disjoint(&expected))
}

fn relevant_count(documents: &[String], qrels: &BTreeMap<String, u8>, cutoff: usize) -> usize {
    documents
        .iter()
        .take(cutoff)
        .filter(|document| qrels.get(*document).is_some_and(|grade| *grade >= 1))
        .count()
}

fn recall(documents: &[String], qrels: &BTreeMap<String, u8>, cutoff: usize) -> f64 {
    let relevant = qrels.values().filter(|grade| **grade >= 1).count();
    relevant_count(documents, qrels, cutoff) as f64 / relevant as f64
}

fn reciprocal_rank(documents: &[String], qrels: &BTreeMap<String, u8>, cutoff: usize) -> f64 {
    documents
        .iter()
        .take(cutoff)
        .position(|document| qrels.get(document).is_some_and(|grade| *grade >= 1))
        .map(|offset| 1.0 / (offset + 1) as f64)
        .unwrap_or(0.0)
}

fn average_precision(documents: &[String], qrels: &BTreeMap<String, u8>) -> f64 {
    let relevant = qrels.values().filter(|grade| **grade >= 1).count();
    let mut found = 0_usize;
    let mut sum = 0.0_f64;
    for (offset, document) in documents.iter().enumerate() {
        if qrels.get(document).is_some_and(|grade| *grade >= 1) {
            found += 1;
            sum += found as f64 / (offset + 1) as f64;
        }
    }
    sum / relevant as f64
}

fn judged(documents: &[String], qrels: &BTreeMap<String, u8>, cutoff: usize) -> f64 {
    let denominator = cutoff.min(documents.len());
    if denominator == 0 {
        return 0.0;
    }
    documents
        .iter()
        .take(cutoff)
        .filter(|document| qrels.contains_key(*document))
        .count() as f64
        / denominator as f64
}

fn ndcg(documents: &[String], qrels: &BTreeMap<String, u8>, cutoff: usize) -> f64 {
    let mut dcg = 0.0_f64;
    for (offset, document) in documents.iter().take(cutoff).enumerate() {
        dcg += gain(*qrels.get(document).unwrap_or(&0), offset);
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

fn validate_selection_path_equality_with_d(
    validated: &ValidatedCollection,
    output: &Path,
    runs: &[RunArtifacts],
) -> Result<Value, String> {
    let d_run_id = |lane: &str| -> Result<&str, String> {
        validated
            .runs
            .iter()
            .find(|run| {
                run.configuration["run_letter"] == "d" && run.configuration["seed_lane"] == lane
            })
            .map(|run| run.run_id.as_str())
            .ok_or_else(|| format!("missing D selection run for lane '{lane}'"))
    };
    let mut rows = Vec::new();
    for run in runs {
        let identity = run
            .result
            .queries
            .first()
            .and_then(|query| query.selection_run_id.as_deref())
            .ok_or_else(|| format!("run '{}' has no selection identity", run.result.run_id))?;
        if identity != run.result.run_id {
            return Err(format!(
                "run '{}' selection identity mismatch",
                run.result.run_id
            ));
        }
        let lane = configured_seed_lane(validated, &run.result.run_id)?;
        let d_run_id = d_run_id(lane)?;
        let d_selections = read_jsonl(
            &output
                .join("graph-selections")
                .join(format!("{d_run_id}.jsonl")),
        )?;
        let d_paths = read_jsonl(&output.join("graph-paths").join(format!("{d_run_id}.jsonl")))?;
        let query_ids = run
            .selection_rows
            .iter()
            .map(|row| row["query_id"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        let expected_selections = d_selections
            .into_iter()
            .filter(|row| query_ids.contains(row["query_id"].as_str().unwrap()))
            .map(normalized_selection)
            .collect::<Result<Vec<_>, _>>()?;
        let actual_selections = run
            .selection_rows
            .iter()
            .cloned()
            .map(normalized_selection)
            .collect::<Result<Vec<_>, _>>()?;
        let expected_paths = d_paths
            .into_iter()
            .filter(|row| query_ids.contains(row["query_id"].as_str().unwrap()))
            .map(normalized_path)
            .collect::<Result<Vec<_>, _>>()?;
        let actual_paths = run
            .path_rows
            .iter()
            .cloned()
            .map(normalized_path)
            .collect::<Result<Vec<_>, _>>()?;
        let equal = actual_selections == expected_selections && actual_paths == expected_paths;
        let invalid = run.result.status == "invalid_execution" || !equal;
        let mut row = json!({
            "d_run_id":d_run_id,
            "path_rows":actual_paths.len(),
            "query_count":actual_selections.len(),
            "run_id":run.result.run_id,
            "selection_equal":equal,
            "path_equal":equal
        });
        if invalid {
            row["status"] = json!("invalid_execution");
        }
        rows.push(row);
    }
    rows.sort_by_key(|row| row["run_id"].as_str().unwrap().to_owned());
    let status = if rows.iter().any(|row| row["status"] == "invalid_execution") {
        "invalid_execution"
    } else {
        "valid"
    };
    Ok(json!({"runs":rows,"schema_version":1,"status":status}))
}

fn configured_seed_lane<'a>(
    validated: &'a ValidatedCollection,
    run_id: &str,
) -> Result<&'a str, String> {
    validated
        .runs
        .iter()
        .find(|run| run.run_id == run_id)
        .and_then(|run| run.configuration["seed_lane"].as_str())
        .ok_or_else(|| format!("run '{run_id}' has no configured seed lane"))
}

fn normalized_selection(mut row: Value) -> Result<String, String> {
    let object = row
        .as_object_mut()
        .ok_or_else(|| "selection row is not an object".to_owned())?;
    object.remove("run_id");
    object.remove("generation_fingerprint");
    canonical_json(&row)
}

fn normalized_path(mut row: Value) -> Result<String, String> {
    row.as_object_mut()
        .ok_or_else(|| "path row is not an object".to_owned())?
        .remove("run_id");
    canonical_json(&row)
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>, String> {
    let bytes = fs::read(path).map_err(|error| format!("read '{}': {error}", path.display()))?;
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_slice(line)
                .map_err(|error| format!("parse '{}': {error}", path.display()))
        })
        .collect()
}

fn validate_frozen_semantic_runs(validated: &ValidatedCollection) -> Result<(), String> {
    if validated.collection.collection_id != "retrievalkit-v3-conformance" {
        return Ok(());
    }
    for (qualification_run_id, logical, lane) in SEMANTIC_RUNS {
        let letter = if qualification_run_id.starts_with("v3-e-") {
            "e"
        } else {
            "f"
        };
        let run = validated
            .runs
            .iter()
            .find(|run| {
                run.configuration["run_letter"] == letter && run.configuration["seed_lane"] == lane
            })
            .ok_or_else(|| {
                format!("missing frozen semantic run for letter '{letter}' lane '{lane}'")
            })?;
        let (declared_hash, execution_hash, execution_count) = match lane {
            "explicit" => (
                "2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f",
                "2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f",
                2,
            ),
            "topic" => (
                "d9bd478b70d090c4b9543d346a42f300977480baf6f7d65f1c30e3608153a082",
                "b64c45f1a2bef306eb3daca23aaa916bcbc151fef367325a7160e9520651f24e",
                2,
            ),
            "team" => (
                "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
                "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
                1,
            ),
            _ => unreachable!(),
        };
        if run.logical_run_sha256 != logical
            || run.declared_hash() != declared_hash
            || run.execution_hash() != execution_hash
            || run.execution.len() != execution_count
        {
            return Err(format!(
                "frozen semantic run '{}' identity or population changed",
                run.run_id
            ));
        }
    }
    Ok(())
}

fn validate_frozen_hybrid_runs(validated: &ValidatedCollection) -> Result<(), String> {
    if validated.collection.collection_id != "retrievalkit-v3-conformance" {
        return Ok(());
    }
    for (_qualification_run_id, logical, lane) in HYBRID_RUNS {
        let run = validated
            .runs
            .iter()
            .find(|run| {
                run.configuration["run_letter"] == "g" && run.configuration["seed_lane"] == lane
            })
            .ok_or_else(|| format!("missing frozen hybrid run for lane '{lane}'"))?;
        let (declared_hash, execution_hash, execution_count) = match lane {
            "explicit" => (
                "2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f",
                "2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f",
                2,
            ),
            "topic" => (
                "d9bd478b70d090c4b9543d346a42f300977480baf6f7d65f1c30e3608153a082",
                "b64c45f1a2bef306eb3daca23aaa916bcbc151fef367325a7160e9520651f24e",
                2,
            ),
            "team" => (
                "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
                "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
                1,
            ),
            _ => unreachable!(),
        };
        if run.logical_run_sha256 != logical
            || run.declared_hash() != declared_hash
            || run.execution_hash() != execution_hash
            || run.execution.len() != execution_count
        {
            return Err(format!(
                "frozen hybrid run '{}' identity or population changed",
                run.run_id
            ));
        }
    }
    Ok(())
}

fn execute_run(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    database: &GraphRetrievalDatabase,
    inputs: &V3ProductionInputs,
    source_queries: &BTreeMap<&str, &Query>,
    seeds: &SeedResolutionSet,
    fingerprint: &str,
) -> Result<RunArtifacts, String> {
    let lane = run.configuration["seed_lane"]
        .as_str()
        .ok_or_else(|| format!("graph retrieval run '{}' has no lane", run.run_id))?;
    let mut result_rows = Vec::new();
    let mut valid = Vec::new();
    let mut observed_failures = ExecutionFailures::default();
    for query_id in &run.declared {
        let source = source_queries
            .get(query_id.as_str())
            .ok_or_else(|| format!("graph retrieval source query '{query_id}' is missing"))?;
        if !run.execution.contains(query_id) {
            result_rows.push(excluded_result(
                source,
                run,
                candidate_limits(run)?,
                excluded_reason(validated, lane, query_id)?,
            ));
            continue;
        }
        let input = inputs
            .queries
            .iter()
            .find(|query| query.query_id == *query_id)
            .ok_or_else(|| format!("graph retrieval production query '{query_id}' is missing"))?;
        let seed = resolved_seed(seeds, lane, query_id)?;
        match execute_query(
            validated,
            database,
            run,
            lane,
            source,
            input,
            seed,
            fingerprint,
        ) {
            Ok(execution) => {
                result_rows.push(execution.result.clone());
                valid.push(execution);
            }
            Err(error) => {
                let (reason, run_wide) = classify_query_failure(&error);
                if run_wide {
                    observed_failures.run(run.run_id.clone(), reason);
                } else {
                    observed_failures.query(run.run_id.clone(), query_id.clone(), reason);
                }
                result_rows.push(QueryExecution {
                    candidate_limits: candidate_limits(run)?,
                    chunk_hits: Vec::new(),
                    duplicate_collapse_count: 0,
                    execution_status: "valid",
                    filter: source.metadata_filter.clone(),
                    projected_documents: Vec::new(),
                    query_id: query_id.clone(),
                    selection_run_id: Some(run.run_id.clone()),
                    status_reason: None,
                });
            }
        }
    }
    result_rows.sort_by(|left, right| left.query_id.cmp(&right.query_id));
    let mut selection_rows = valid
        .iter()
        .map(|query| query.selection_row.clone())
        .collect::<Vec<_>>();
    selection_rows.sort_by_key(|row| row["query_id"].as_str().unwrap().to_owned());
    let mut path_rows = valid
        .iter()
        .flat_map(|query| query.path_rows.clone())
        .collect::<Vec<_>>();
    path_rows.sort_by_key(path_sort_key);
    let mut projection_rows = valid
        .into_iter()
        .map(|query| query.projection_row)
        .collect::<Vec<_>>();
    projection_rows.sort_by_key(|row| row["query_id"].as_str().unwrap().to_owned());
    let mut artifacts = RunArtifacts {
        result: RunExecution {
            queries: result_rows,
            run_id: run.run_id.clone(),
            status: "valid",
        },
        path_rows,
        projection_rows,
        selection_rows,
    };
    apply_failures(run, &mut artifacts, &observed_failures);
    Ok(artifacts)
}

fn apply_failures(run: &RunIdentity, artifacts: &mut RunArtifacts, failures: &ExecutionFailures) {
    if !failures.run_is_invalid(&run.run_id) {
        return;
    }
    let mut invalid_queries = BTreeSet::new();
    for result in &mut artifacts.result.queries {
        if result.execution_status == "excluded_pre_freeze" {
            continue;
        }
        if let Some(reason) = failures.reason_for(&run.run_id, &result.query_id) {
            result.execution_status = "invalid_execution";
            result.status_reason = Some(reason.as_str().to_owned());
            result.chunk_hits.clear();
            result.projected_documents.clear();
            result.duplicate_collapse_count = 0;
            invalid_queries.insert(result.query_id.clone());
        }
    }
    artifacts
        .selection_rows
        .retain(|row| !invalid_queries.contains(row["query_id"].as_str().unwrap()));
    artifacts
        .path_rows
        .retain(|row| !invalid_queries.contains(row["query_id"].as_str().unwrap()));
    artifacts
        .projection_rows
        .retain(|row| !invalid_queries.contains(row["query_id"].as_str().unwrap()));
    artifacts.result.status = if invalid_queries.is_empty() {
        "valid"
    } else {
        "invalid_execution"
    };
}

#[allow(clippy::too_many_arguments)]
fn execute_query(
    validated: &ValidatedCollection,
    database: &GraphRetrievalDatabase,
    run: &RunIdentity,
    lane: &str,
    query: &Query,
    input: &ProductionQueryInput,
    seed: &ResolvedSeed,
    fingerprint: &str,
) -> Result<ValidQuery, String> {
    let graph_result = database
        .graph_query(&production_query(query, seed)?, None)
        .map_err(|error| {
            format!(
                "graph retrieval query '{}': traversal: {error}",
                query.query_id
            )
        })?;
    let projected = database
        .project_candidates(&graph_result)
        .map_err(|error| {
            format!(
                "graph retrieval query '{}': projection: {error}",
                query.query_id
            )
        })?;
    let filter = query
        .metadata_filter
        .as_ref()
        .map(convert_filter)
        .transpose()?;
    let filtered_projection = database
        .project_candidate_identities(&graph_result, filter.as_ref())
        .map_err(|error| {
            format!(
                "graph retrieval query '{}': filtered projection: {error}",
                query.query_id
            )
        })?;
    if projected.trace.resolved_chunks != filtered_projection.projected_chunks_before_filter {
        return Err(format!(
            "graph retrieval query '{}': projection count mismatch",
            query.query_id
        ));
    }

    let allowed = filtered_projection
        .candidates
        .iter()
        .map(|identity| {
            (
                identity.record_id.as_str().to_owned(),
                identity.chunk_key.as_str().to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();
    let limits = candidate_limits(run)?;
    let chunk_hits = match run.configuration["run_letter"].as_str() {
        Some("e" | "f") => semantic_hits(database, input, &graph_result, &allowed)?,
        Some("g") => hybrid_hits(database, input, &graph_result, &allowed, limits, run)?,
        actual => {
            return Err(format!(
                "graph retrieval run '{}' has unsupported letter {actual:?}",
                run.run_id
            ));
        }
    };
    validate_native_hits(&chunk_hits, run, &query.query_id)?;
    let (projected_documents, duplicate_collapse_count) =
        project_documents(&chunk_hits, validated.collection.evaluation_depth);

    let all_scope = database
        .corpus()
        .candidate_scope(
            database
                .corpus()
                .chunk_identities()
                .map(|(_, chunk_id)| chunk_id),
        )
        .map_err(|error| {
            format!(
                "graph retrieval query '{}': all scope: {error}",
                query.query_id
            )
        })?;
    let eligible = database
        .corpus()
        .filter_candidate_scope(&all_scope, filter.as_ref())
        .map_err(|error| {
            format!(
                "graph retrieval query '{}': eligible filter: {error}",
                query.query_id
            )
        })?;
    let candidate_documents = filtered_projection
        .candidates
        .iter()
        .map(|identity| identity.record_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut matched_nodes = graph_result
        .matches
        .iter()
        .map(|matched| canonical_node(&matched.node_id))
        .collect::<Vec<_>>();
    matched_nodes.sort_by_key(|node| canonical_json(node).unwrap());
    let mut path_rows = graph_result
        .matches
        .iter()
        .map(|matched| path_row(&query.query_id, &run.run_id, matched))
        .collect::<Result<Vec<_>, _>>()?;
    path_rows.sort_by_key(path_sort_key);
    let selection_row = json!({
        "active_corpus_chunks_before_filter":database.corpus().active_chunk_count(),
        "corpus_id":database.corpus().corpus_id().as_str(),
        "eligible_corpus_chunks_after_filter":eligible.len(),
        "generation_fingerprint":fingerprint,
        "matched_nodes":matched_nodes,
        "projected_chunks_after_filter":filtered_projection.projected_chunks_after_filter,
        "projected_chunks_before_filter":filtered_projection.projected_chunks_before_filter,
        "projected_documents_after_filter":candidate_documents.len(),
        "query_id":query.query_id,
        "resolved_seed":seed.canonical,
        "run_id":run.run_id,
        "seed_lane":lane,
        "seed_provenance":seed.provenance,
        "seed_status":"resolved",
        "stale":false,
        "trace":{
            "diagnostics":graph_result.trace.diagnostics,
            "result_count":graph_result.trace.result_count,
            "seed_count":graph_result.trace.seed_count,
            "traversed_edges":graph_result.trace.traversed_edges,
            "visited_states":graph_result.trace.visited_states
        },
        "truncated_reason":truncation_name(graph_result.truncated)
    });
    let projection_row = json!({
        "candidates":filtered_projection.candidates.iter().map(canonical_identity).collect::<Vec<_>>(),
        "query_id":query.query_id,
        "run_id":run.run_id
    });
    Ok(ValidQuery {
        result: QueryExecution {
            candidate_limits: limits,
            chunk_hits,
            duplicate_collapse_count,
            execution_status: "valid",
            filter: query.metadata_filter.clone(),
            projected_documents,
            query_id: query.query_id.clone(),
            selection_run_id: Some(run.run_id.clone()),
            status_reason: None,
        },
        path_rows,
        projection_row,
        selection_row,
    })
}

fn excluded_result(
    query: &Query,
    run: &RunIdentity,
    limits: CandidateLimits,
    reason: String,
) -> QueryExecution {
    QueryExecution {
        candidate_limits: limits,
        chunk_hits: Vec::new(),
        duplicate_collapse_count: 0,
        execution_status: "excluded_pre_freeze",
        filter: query.metadata_filter.clone(),
        projected_documents: Vec::new(),
        query_id: query.query_id.clone(),
        selection_run_id: Some(run.run_id.clone()),
        status_reason: Some(reason),
    }
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
                .ok_or_else(|| format!("run '{}' has invalid {name} limit", run.run_id))
        }
    };
    Ok(CandidateLimits {
        keyword: read("keyword")?,
        vector: read("vector")?,
    })
}

fn semantic_hits(
    database: &GraphRetrievalDatabase,
    input: &ProductionQueryInput,
    graph_result: &retrievalkit_graph::GraphResult,
    allowed: &BTreeSet<(String, String)>,
) -> Result<Vec<ChunkHit>, String> {
    let mut request = SearchQuery::new(
        input.embedding.clone(),
        database.corpus().active_chunk_count(),
    );
    if let Some(filter) = &input.filter {
        request = request.with_filter(filter.clone());
    }
    database
        .semantic_search_in_selection(&request, graph_result)
        .map_err(|error| {
            format!(
                "graph retrieval query '{}': scoped semantic ranking: {error}",
                input.query_id
            )
        })?
        .into_iter()
        .enumerate()
        .map(|(offset, hit)| {
            let identity = database
                .corpus()
                .chunk_identity(hit.chunk_id)
                .ok_or_else(|| {
                    format!("scoped semantic returned unknown chunk {}", hit.chunk_id)
                })?;
            let stable = (
                identity.record_id.as_str().to_owned(),
                identity.chunk_key.as_str().to_owned(),
            );
            if !allowed.contains(&stable)
                || hit.document_id != identity.record_id.as_str()
                || !hit.score.is_finite()
            {
                return Err(format!(
                    "graph retrieval query '{}': out-of-scope or invalid semantic hit",
                    input.query_id
                ));
            }
            Ok(ChunkHit {
                bm25_normalized_score: None,
                bm25_score: None,
                chunk_key: stable.1,
                fusion_score: None,
                keyword_rank: None,
                matched_terms: Vec::new(),
                native_rank: offset + 1,
                record_id: stable.0,
                vector_normalized_score: None,
                vector_rank: Some(offset + 1),
                vector_score: Some(hit.score),
            })
        })
        .collect()
}

fn hybrid_hits(
    database: &GraphRetrievalDatabase,
    input: &ProductionQueryInput,
    graph_result: &retrievalkit_graph::GraphResult,
    allowed: &BTreeSet<(String, String)>,
    limits: CandidateLimits,
    run: &RunIdentity,
) -> Result<Vec<ChunkHit>, String> {
    let vector_limit = limits
        .vector
        .ok_or_else(|| format!("run '{}' lacks vector candidate limit", run.run_id))?;
    let keyword_limit = limits
        .keyword
        .ok_or_else(|| format!("run '{}' lacks keyword candidate limit", run.run_id))?;
    let alpha = run.configuration["fusion_alpha"]
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| format!("run '{}' has invalid fusion alpha", run.run_id))?;
    let mut request = HybridQuery::new(
        input.text.clone(),
        input.embedding.clone(),
        vector_limit.saturating_add(keyword_limit),
    )
    .with_candidate_limits(vector_limit, keyword_limit)
    .with_alpha(alpha);
    if let Some(filter) = &input.filter {
        request = request.with_filter(filter.clone());
    }
    let mut hits = database
        .hybrid_search_in_selection(&request, graph_result)
        .map_err(|error| {
            format!(
                "graph retrieval query '{}': scoped hybrid ranking: {error}",
                input.query_id
            )
        })?
        .into_iter()
        .enumerate()
        .map(|(offset, hit)| convert_hybrid_hit(database, input, allowed, offset, hit))
        .collect::<Result<Vec<_>, _>>()?;
    canonicalize_equal_score_ties(&mut hits);
    Ok(hits)
}

fn canonicalize_equal_score_ties(hits: &mut [ChunkHit]) {
    hits.sort_by(|left, right| {
        right
            .fusion_score
            .expect("weighted hit has fusion score")
            .total_cmp(&left.fusion_score.expect("weighted hit has fusion score"))
            .then_with(|| {
                (&left.record_id, &left.chunk_key).cmp(&(&right.record_id, &right.chunk_key))
            })
    });
    for (offset, hit) in hits.iter_mut().enumerate() {
        hit.native_rank = offset + 1;
    }
}

fn convert_hybrid_hit(
    database: &GraphRetrievalDatabase,
    input: &ProductionQueryInput,
    allowed: &BTreeSet<(String, String)>,
    offset: usize,
    hit: HybridHit,
) -> Result<ChunkHit, String> {
    let identity = database
        .corpus()
        .chunk_identity(hit.chunk_id)
        .ok_or_else(|| format!("scoped hybrid returned unknown chunk {}", hit.chunk_id))?;
    let stable = (
        identity.record_id.as_str().to_owned(),
        identity.chunk_key.as_str().to_owned(),
    );
    if !allowed.contains(&stable)
        || hit.document_id != identity.record_id.as_str()
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
        || hit.vector_score.is_some() != hit.trace.normalized_vector_score.is_some()
        || hit.keyword_score.is_some() != hit.trace.normalized_keyword_score.is_some()
        || hit.vector_score.is_some() != hit.trace.vector_rank.is_some()
        || hit.keyword_score.is_some() != hit.trace.keyword_rank.is_some()
        || (hit.trace.keyword_rank.is_none() && !hit.trace.matched_terms.is_empty())
    {
        return Err(format!(
            "graph retrieval query '{}': invalid scoped hybrid hit or trace",
            input.query_id
        ));
    }
    Ok(ChunkHit {
        bm25_normalized_score: hit.trace.normalized_keyword_score,
        bm25_score: hit.keyword_score,
        chunk_key: stable.1,
        fusion_score: Some(hit.score),
        keyword_rank: hit.trace.keyword_rank,
        matched_terms: hit.trace.matched_terms,
        native_rank: offset + 1,
        record_id: stable.0,
        vector_normalized_score: hit.trace.normalized_vector_score,
        vector_rank: hit.trace.vector_rank,
        vector_score: hit.vector_score,
    })
}

fn production_query(query: &Query, seed: &ResolvedSeed) -> Result<GraphQuery, String> {
    let mut result = GraphQuery::new(seed.production.clone());
    for step in &query.traversal.steps {
        result = result.traverse(Traverse {
            relationship: RelationshipType::new(step.relationship_type.clone())
                .map_err(|error| format!("graph retrieval traversal relationship: {error}"))?,
            direction: match step.direction.as_str() {
                "outgoing" => Direction::Outgoing,
                "incoming" => Direction::Incoming,
                actual => return Err(format!("graph retrieval direction '{actual}' is invalid")),
            },
            min_hops: step.min_hops,
            max_hops: step.max_hops,
        });
    }
    Ok(result.with_limits(QueryLimits {
        max_hops: query.traversal.limits.max_hops,
        max_visited: query.traversal.limits.max_visited,
        max_results: query.traversal.limits.max_results,
        max_working_bytes: query.traversal.limits.max_working_bytes,
    }))
}

fn resolved_seed<'a>(
    seeds: &'a SeedResolutionSet,
    lane: &str,
    query_id: &str,
) -> Result<&'a ResolvedSeed, String> {
    if lane == "explicit" {
        return seeds
            .explicit
            .get(query_id)
            .ok_or_else(|| format!("explicit seed missing for '{query_id}'"));
    }
    match seeds.derived.get(&(lane.to_owned(), query_id.to_owned())) {
        Some(DerivedSeedOutcome::Resolved(seed)) => Ok(seed),
        Some(DerivedSeedOutcome::Excluded(reason)) => Err(format!(
            "excluded seed '{lane}/{query_id}' unexpectedly executed: {reason}"
        )),
        None => Err(format!("derived seed missing for '{lane}/{query_id}'")),
    }
}

fn excluded_reason(
    validated: &ValidatedCollection,
    lane: &str,
    query_id: &str,
) -> Result<String, String> {
    validated
        .exclusions
        .iter()
        .find(|row| row.lane == lane && row.query_id == query_id)
        .map(|row| row.reason.clone())
        .ok_or_else(|| format!("missing frozen exclusion for '{lane}/{query_id}'"))
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
                "graph retrieval run '{}' query '{}': duplicate or non-consecutive hit",
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
                "graph retrieval run '{}' query '{}': native ordering mismatch",
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

fn canonical_node(node: &NodeId) -> Value {
    let source = match &node.source {
        NodeSource::Record(record_id) => {
            json!({"kind":"record","record_id":record_id.as_str()})
        }
        NodeSource::Chunk(identity) => json!({
            "chunk_key":identity.chunk_key.as_str(),
            "kind":"chunk",
            "record_id":identity.record_id.as_str()
        }),
    };
    json!({"node_type":node.node_type.as_str(),"source":source})
}

fn canonical_identity(identity: &retrievalkit_core::ChunkIdentity) -> Value {
    json!({
        "chunk_key":identity.chunk_key.as_str(),
        "record_id":identity.record_id.as_str()
    })
}

fn path_row(
    query_id: &str,
    run_id: &str,
    matched: &retrievalkit_graph::GraphMatch,
) -> Result<Value, String> {
    let edges = canonical_path(&matched.node_id, &matched.path)?;
    Ok(json!({
        "depth":edges.len(),
        "edges":edges,
        "matched_node":canonical_node(&matched.node_id),
        "path_ordinal":0,
        "query_id":query_id,
        "run_id":run_id
    }))
}

fn canonical_path(matched: &NodeId, path: &[GraphPathEdge]) -> Result<Vec<Value>, String> {
    let mut current = matched;
    let mut directions = Vec::with_capacity(path.len());
    for edge in path.iter().rev() {
        if &edge.edge_id.target == current {
            directions.push("outgoing");
            current = &edge.edge_id.source;
        } else if &edge.edge_id.source == current {
            directions.push("incoming");
            current = &edge.edge_id.target;
        } else {
            return Err("graph retrieval path is not contiguous".to_owned());
        }
    }
    directions.reverse();
    Ok(path
        .iter()
        .zip(directions)
        .map(|(edge, direction)| {
            json!({
                "direction":direction,
                "occurrence_ordinal":edge.edge_id.occurrence_ordinal,
                "relationship_type":edge.edge_id.relationship_type.as_str(),
                "source_node":canonical_node(&edge.edge_id.source),
                "target_node":canonical_node(&edge.edge_id.target)
            })
        })
        .collect())
}

fn truncation_name(reason: Option<TruncationReason>) -> Option<&'static str> {
    reason.map(|reason| match reason {
        TruncationReason::MaxHops => "max_hops",
        TruncationReason::MaxVisited => "max_visited",
        TruncationReason::MaxResults => "max_results",
        TruncationReason::MaxWorkingBytes => "max_working_bytes",
    })
}

fn path_sort_key(value: &Value) -> (String, String, String) {
    (
        value["query_id"].as_str().unwrap().to_owned(),
        canonical_json(&value["matched_node"]).unwrap(),
        canonical_json(&value["edges"]).unwrap(),
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

fn write_jsonl(path: &Path, rows: &[Value]) -> Result<(), String> {
    let mut bytes = Vec::new();
    for row in rows {
        bytes.extend(canonical_json_line(row)?);
    }
    fs::write(path, bytes).map_err(|error| format!("write '{}': {error}", path.display()))
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(prefix: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_nanos();
        let counter = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("{prefix}-{}-{nonce}-{counter}", std::process::id()));
        fs::create_dir(&path).map_err(|error| {
            format!(
                "failed to create graph retrieval persistence directory '{}': {error}",
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
    use std::path::PathBuf;

    use retrievalkit_core::{CorpusId, RecordChunkInput};
    use retrievalkit_graph::GraphError;

    use super::*;
    use crate::quality::v3_graph_input::production_schema;
    use crate::quality::v3_validation::validate;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    #[test]
    fn configured_seed_lane_supports_collection_defined_lane_names() {
        let mut validated = validate(&fixture_root()).unwrap();
        let run = validated
            .runs
            .iter_mut()
            .find(|run| run.configuration["run_letter"] == "e")
            .unwrap();
        let run_id = run.run_id.clone();
        run.configuration["seed_lane"] = json!("hotpotqa-exact-title-v1");
        assert_eq!(
            configured_seed_lane(&validated, &run_id).unwrap(),
            "hotpotqa-exact-title-v1"
        );
    }

    #[test]
    fn executes_nine_frozen_graph_retrieval_runs_through_own_combined_databases() {
        let validated = validate(&fixture_root()).unwrap();
        validate_frozen_semantic_runs(&validated).unwrap();
        validate_frozen_hybrid_runs(&validated).unwrap();
        let seeds = resolve_seeds(&validated).unwrap();
        let inputs = V3ProductionInputs::from_validated(&validated).unwrap();
        let source_queries = validated
            .queries
            .iter()
            .map(|query| (query.query_id.as_str(), query))
            .collect::<BTreeMap<_, _>>();
        let mut valid_executions = 0;
        let mut exclusions = 0;
        for run in validated.runs.iter().filter(|run| {
            matches!(
                run.configuration["run_letter"].as_str(),
                Some("e" | "f" | "g")
            )
        }) {
            let letter = run.configuration["run_letter"].as_str().unwrap();
            let encoding = match letter {
                "e" => VectorEncoding::F32,
                "f" | "g" => VectorEncoding::I8ScalarQuantized,
                _ => unreachable!(),
            };
            let database = build_graph_retrieval_database(&validated, encoding).unwrap();
            let (_, fingerprint) = retrieval_generation_fingerprint(&validated, letter).unwrap();
            let artifacts = execute_run(
                &validated,
                run,
                &database,
                &inputs,
                &source_queries,
                &seeds,
                &fingerprint,
            )
            .unwrap();
            valid_executions += artifacts.selection_rows.len();
            exclusions += artifacts
                .result
                .queries
                .iter()
                .filter(|query| query.execution_status == "excluded_pre_freeze")
                .count();
            assert!(artifacts.result.queries.iter().all(|query| {
                query.execution_status != "valid"
                    || query.chunk_hits.iter().all(|hit| {
                        if letter == "g" {
                            hit.fusion_score.is_some()
                        } else {
                            hit.vector_score.is_some() && hit.fusion_score.is_none()
                        }
                    })
            }));
        }
        assert_eq!(valid_executions, 15);
        assert_eq!(exclusions, 6);
    }

    #[test]
    fn persisted_e_to_g_runs_recreate_identical_selections_paths_and_rankings() {
        let validated = validate(&fixture_root()).unwrap();
        let seeds = resolve_seeds(&validated).unwrap();
        let inputs = V3ProductionInputs::from_validated(&validated).unwrap();
        let source_queries = validated
            .queries
            .iter()
            .map(|query| (query.query_id.as_str(), query))
            .collect::<BTreeMap<_, _>>();
        for run in validated.runs.iter().filter(|run| {
            matches!(
                run.configuration["run_letter"].as_str(),
                Some("e" | "f" | "g")
            )
        }) {
            let letter = run.configuration["run_letter"].as_str().unwrap();
            let encoding = match letter {
                "e" => VectorEncoding::F32,
                "f" | "g" => VectorEncoding::I8ScalarQuantized,
                _ => unreachable!(),
            };
            let database = build_graph_retrieval_database(&validated, encoding).unwrap();
            let (_, fingerprint) = retrieval_generation_fingerprint(&validated, letter).unwrap();
            let (artifacts, persistence) = execute_run_with_persistence(
                &validated,
                run,
                &database,
                &inputs,
                &source_queries,
                &seeds,
                &fingerprint,
            )
            .unwrap();
            assert_eq!(artifacts.result.status, "valid");
            assert!(persistence.save_validate_load_equivalent);
            assert!(persistence.selection_equal);
            assert!(persistence.path_equal);
            assert!(persistence.projection_equal);
            assert!(persistence.ranking_equal);
        }
    }

    #[test]
    fn combined_database_rejects_stale_and_cross_corpus_selections() {
        let validated = validate(&fixture_root()).unwrap();
        let seeds = resolve_seeds(&validated).unwrap();
        let query = validated
            .queries
            .iter()
            .find(|query| query.query_id == "qb")
            .unwrap();
        let seed = resolved_seed(&seeds, "explicit", "qb").unwrap();
        let database = build_graph_retrieval_database(&validated, VectorEncoding::F32).unwrap();
        let selection = database
            .graph_query(&production_query(query, seed).unwrap(), None)
            .unwrap();

        let mut cross_inputs = V3ProductionInputs::from_validated(&validated).unwrap();
        cross_inputs.corpus_id = CorpusId::new("v3-cross-corpus").unwrap();
        let cross_database = GraphRetrievalDatabase::build(
            cross_inputs.build_database(VectorEncoding::F32).unwrap(),
            production_schema(&validated.graph_schema).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            cross_database.project_candidates(&selection),
            Err(GraphError::StaleGeneration { .. })
        ));

        let newer_inputs = V3ProductionInputs::from_validated(&validated).unwrap();
        let mut newer_retrieval = newer_inputs.build_database(VectorEncoding::F32).unwrap();
        let replacement = &newer_inputs.records[0];
        let replacement_chunks = replacement
            .chunks
            .iter()
            .map(|chunk| RecordChunkInput {
                key: chunk.key.clone(),
                text: chunk.text.clone(),
                embedding: vec![1.0, 0.0, 0.0],
                metadata: chunk.metadata.clone(),
            })
            .collect();
        newer_retrieval
            .upsert_record(
                replacement.record.clone(),
                replacement.inherited_metadata.clone(),
                replacement_chunks,
            )
            .unwrap();
        let newer_database = GraphRetrievalDatabase::build(
            newer_retrieval,
            production_schema(&validated.graph_schema).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            newer_database.project_candidates(&selection),
            Err(GraphError::StaleGeneration { .. })
        ));
    }
}
