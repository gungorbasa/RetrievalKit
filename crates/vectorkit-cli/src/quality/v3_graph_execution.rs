use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Map, Value};
use vectorkit_core::ChunkIdentity;
use vectorkit_graph::{
    Direction, GraphDatabase, GraphPathEdge, GraphQuery, NodeId, NodeSource, QueryLimits,
    RelationshipType, Traverse, TruncationReason,
};

use super::v3::d_generation_fingerprint;
use super::v3_canonical::{canonical_json, canonical_json_line, write_canonical_json};
use super::v3_execution_status::{classify_query_failure, ExecutionFailures, FailureReason};
use super::v3_graph_input::build_graph_database;
use super::v3_ingestion::convert_filter;
use super::v3_population::population_hash;
use super::v3_runs::RunIdentity;
use super::v3_schema::{EvidenceJudgment, ExpectedPaths, Query};
use super::v3_seed::{resolve_seeds, DerivedSeedOutcome, ResolvedSeed, SeedResolutionSet};
use super::v3_validation::ValidatedCollection;

const D_METRICS: [&str; 24] = [
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
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);
const D_RUNS: [(&str, &str, &str, &str, &str, usize); 3] = [
    (
        "explicit",
        "v3-d-selection-none-none-explicit-cfg-13feb2a18ac3",
        "1bedbc6a99c164ed8ab69287192bf7287577eeb278406b9475cf3232bb2b0bde",
        "533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5",
        "533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5",
        3,
    ),
    (
        "topic",
        "v3-d-selection-none-none-topic-cfg-bf6bed5c72e7",
        "03e34447316a451bb023fb82635d0c91dee8f343e37eab909697528e2095302a",
        "a3b85dfbb4d7e5178e8cf34ab7c8d1474fbc03ceba933c731fbb83da012ad2f8",
        "be40e5a59829766e4ec9bc36e50f69f2c3f0b8c4f0e59fff0f253878622bac59",
        3,
    ),
    (
        "team",
        "v3-d-selection-none-none-team-cfg-7278e2315c8f",
        "2c7850eb3ca1c9258765ff9b7dd338d00387e3132b6a4e5380bbac072d38c1aa",
        "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
        "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
        1,
    ),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct GraphQualificationResults {
    collection_id: String,
    collection_version: String,
    runs: Vec<GraphRunExecution>,
    schema_version: u8,
    seed_resolutions: Value,
}

impl GraphQualificationResults {
    pub(super) fn has_invalid_execution(&self) -> bool {
        self.runs
            .iter()
            .any(|run| run.status == "invalid_execution")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct GraphRunExecution {
    queries: Vec<GraphQueryExecution>,
    run_id: String,
    status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct GraphQueryExecution {
    candidate_limits: CandidateLimits,
    chunk_hits: Vec<Value>,
    duplicate_collapse_count: usize,
    execution_status: &'static str,
    filter: Option<Value>,
    projected_documents: Vec<Value>,
    query_id: String,
    selection_run_id: Option<String>,
    status_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct CandidateLimits {
    keyword: Option<usize>,
    vector: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct ValidGraphQuery {
    candidate_documents: BTreeSet<String>,
    candidate_identities: Vec<Value>,
    eligible_chunks: usize,
    metrics: BTreeMap<&'static str, Metric>,
    path_rows: Vec<Value>,
    projected_chunks: usize,
    selection_row: Value,
}

struct QueryMetricContext<'a> {
    validated: &'a ValidatedCollection,
    lane: &'a str,
    query: &'a Query,
    documents: &'a BTreeSet<String>,
    eligible_chunks: usize,
    projected_chunks: usize,
    path_rows: &'a [Value],
    truncated: Option<TruncationReason>,
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

#[derive(Debug, Clone, PartialEq)]
struct RunArtifacts {
    result: GraphRunExecution,
    metrics: Value,
    path_rows: Vec<Value>,
    projection_rows: Vec<Value>,
    selection_rows: Vec<Value>,
    valid: BTreeMap<String, ValidGraphQuery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PersistenceValidation {
    run_id: String,
    save_validate_load_equivalent: bool,
    stable_generation_fingerprint: String,
}

pub(super) fn emit_graph_qualification_with_failures(
    validated: &ValidatedCollection,
    output: &Path,
    failures: &ExecutionFailures,
) -> Result<GraphQualificationResults, String> {
    let seeds = resolve_seeds(validated)?;
    validate_frozen_runs(validated)?;
    let (fingerprint_preimage, fingerprint) = d_generation_fingerprint(validated)?;
    let mut runs = Vec::new();
    let mut persistence = Vec::new();
    for run in validated
        .runs
        .iter()
        .filter(|run| run.configuration["run_letter"] == "d")
    {
        let database = build_graph_database(validated)?;
        let (artifacts, validation) = execute_run_with_persistence_and_failures(
            validated,
            run,
            &database,
            &seeds,
            &fingerprint,
            failures,
        )?;
        runs.push(artifacts);
        persistence.push(validation);
    }
    runs.sort_by(|left, right| left.result.run_id.cmp(&right.result.run_id));
    persistence.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    if runs.len() != 3 {
        return Err(format!(
            "V3 Phase 1.2b expected exactly three D runs, actual {}",
            runs.len()
        ));
    }

    fs::create_dir_all(output.join("graph-selections"))
        .map_err(|error| format!("create graph selection artifacts: {error}"))?;
    fs::create_dir_all(output.join("graph-paths"))
        .map_err(|error| format!("create graph path artifacts: {error}"))?;
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
        projection_rows.extend(run.projection_rows.clone());
    }
    projection_rows.sort_by_key(|row| {
        (
            row["run_id"].as_str().unwrap().to_owned(),
            row["query_id"].as_str().unwrap().to_owned(),
        )
    });
    write_jsonl(
        &output.join("graph-projection-identities.jsonl"),
        &projection_rows,
    )?;
    let seed_resolutions = serde_json::to_value(&seeds.diagnostics)
        .map_err(|error| format!("encode seed diagnostics: {error}"))?;
    let results = GraphQualificationResults {
        collection_id: validated.collection.collection_id.clone(),
        collection_version: validated.collection.collection_version.clone(),
        runs: runs.iter().map(|run| run.result.clone()).collect(),
        schema_version: 3,
        seed_resolutions: seed_resolutions.clone(),
    };
    write_canonical_json(
        &output.join("graph-rust-results.json"),
        &serde_json::to_value(&results)
            .map_err(|error| format!("encode graph Rust results: {error}"))?,
    )?;
    write_canonical_json(
        &output.join("graph-metrics.json"),
        &json!({
            "collection_id":validated.collection.collection_id,
            "collection_version":validated.collection.collection_version,
            "generation_fingerprint":fingerprint,
            "metric_definition_version":"graph-retrieval-v3-r2",
            "partial":true,
            "publication_ready":false,
            "runs":runs.iter().map(|run|run.metrics.clone()).collect::<Vec<_>>(),
            "schema_version":3
        }),
    )?;
    write_canonical_json(
        &output.join("seed-resolution-diagnostics.json"),
        &json!({"schema_version":3,"seed_resolutions":seed_resolutions}),
    )?;
    write_canonical_json(
        &output.join("graph-generation-fingerprint.json"),
        &json!({"fingerprint":fingerprint,"preimage":fingerprint_preimage,"schema_version":1}),
    )?;
    write_canonical_json(
        &output.join("graph-persistence-validation.json"),
        &json!({
            "runs":persistence,
            "schema_version":1,
            "status":if runs.iter().any(|run|run.result.status=="invalid_execution") {"invalid_execution"} else {"valid"}
        }),
    )?;
    Ok(results)
}

#[cfg(test)]
fn execute_run_with_persistence(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    database: &GraphDatabase,
    seeds: &SeedResolutionSet,
    fingerprint: &str,
) -> Result<(RunArtifacts, PersistenceValidation), String> {
    execute_run_with_persistence_and_failures(
        validated,
        run,
        database,
        seeds,
        fingerprint,
        &ExecutionFailures::default(),
    )
}

fn execute_run_with_persistence_and_failures(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    database: &GraphDatabase,
    seeds: &SeedResolutionSet,
    fingerprint: &str,
    injected_failures: &ExecutionFailures,
) -> Result<(RunArtifacts, PersistenceValidation), String> {
    let mut before = execute_run(validated, run, database, seeds, fingerprint)?;
    let repeated = execute_run(validated, run, database, seeds, fingerprint)?;
    let mut failures = injected_failures.clone();
    if before != repeated {
        failures.run(run.run_id.clone(), FailureReason::NonDeterministicRanking);
    }
    let temporary = TemporaryDirectory::new("vectorkit-v3-phase-1-2b-persistence")?;
    let persisted = temporary.path.join("database");
    database
        .save_to_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2b save '{}': {error}", run.run_id))?;
    GraphDatabase::validate_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2b validate '{}': {error}", run.run_id))?;
    let loaded = GraphDatabase::load_from_dir(&persisted)
        .map_err(|error| format!("V3 Phase 1.2b reload '{}': {error}", run.run_id))?;
    if verify_persisted_database(database, &loaded, run).is_err() {
        failures.run(run.run_id.clone(), FailureReason::ReloadMismatch);
    }
    let after = execute_run(validated, run, &loaded, seeds, fingerprint)?;
    if before != after {
        failures.run(run.run_id.clone(), FailureReason::PersistenceMismatch);
    }
    apply_failures(validated, run, &mut before, &failures)?;
    let persistence_equivalent = !matches!(
        failures.run_reason(&run.run_id),
        Some(FailureReason::PersistenceMismatch | FailureReason::ReloadMismatch)
    );
    Ok((
        before,
        PersistenceValidation {
            run_id: run.run_id.clone(),
            save_validate_load_equivalent: persistence_equivalent,
            stable_generation_fingerprint: fingerprint.to_owned(),
        },
    ))
}

fn verify_persisted_database(
    before: &GraphDatabase,
    after: &GraphDatabase,
    run: &RunIdentity,
) -> Result<(), String> {
    let stable_identities = |database: &GraphDatabase| {
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
        || before.graph().schema() != after.graph().schema()
        || before.graph().build_stats() != after.graph().build_stats()
        || before.graph().node_count() != after.graph().node_count()
        || before.graph().edge_count() != after.graph().edge_count()
    {
        return Err(format!(
            "V3 Phase 1.2b reload_mismatch for run '{}'",
            run.run_id
        ));
    }
    Ok(())
}

fn validate_frozen_runs(validated: &ValidatedCollection) -> Result<(), String> {
    for (lane, _qualification_run_id, logical, declared_hash, execution_hash, execution_count) in
        D_RUNS
    {
        let run = validated
            .runs
            .iter()
            .find(|run| {
                run.configuration["run_letter"] == "d" && run.configuration["seed_lane"] == lane
            })
            .ok_or_else(|| format!("missing frozen D lane '{lane}'"))?;
        if run.logical_run_sha256 != logical
            || run.declared_hash() != declared_hash
            || run.execution_hash() != execution_hash
            || run.execution.len() != execution_count
        {
            return Err(format!(
                "frozen D lane '{lane}' identity or population changed: run={}, logical={}, declared_hash={}, execution_hash={}, count={}",
                run.run_id,
                run.logical_run_sha256,
                run.declared_hash(),
                run.execution_hash(),
                run.execution.len()
            ));
        }
    }
    Ok(())
}

fn execute_run(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    database: &GraphDatabase,
    seeds: &SeedResolutionSet,
    fingerprint: &str,
) -> Result<RunArtifacts, String> {
    let lane = run.configuration["seed_lane"]
        .as_str()
        .ok_or_else(|| format!("D run '{}' has no seed lane", run.run_id))?;
    let queries = validated
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let mut result_rows = Vec::new();
    let mut valid = BTreeMap::<String, ValidGraphQuery>::new();
    let mut observed_failures = ExecutionFailures::default();
    for query_id in &run.declared {
        let query = queries
            .get(query_id.as_str())
            .ok_or_else(|| format!("D run references missing query '{query_id}'"))?;
        if !run.execution.contains(query_id) {
            let reason = excluded_reason(validated, lane, query_id)?;
            result_rows.push(result_row(query, run, "excluded_pre_freeze", Some(reason)));
            continue;
        }
        let seed = resolved_seed(seeds, lane, query_id)?;
        match execute_query(validated, database, run, lane, query, seed, fingerprint) {
            Ok(execution) => {
                result_rows.push(result_row(query, run, "valid", None));
                valid.insert(query_id.clone(), execution);
            }
            Err(error) => {
                let (reason, run_wide) = classify_query_failure(&error);
                if run_wide {
                    observed_failures.run(run.run_id.clone(), reason);
                } else {
                    observed_failures.query(run.run_id.clone(), query_id.clone(), reason);
                }
                result_rows.push(result_row(query, run, "valid", None));
            }
        }
    }
    result_rows.sort_by(|left, right| left.query_id.cmp(&right.query_id));

    let metrics = run_metrics(validated, run, &valid, &result_rows)?;
    let mut selection_rows = valid
        .values()
        .map(|query| query.selection_row.clone())
        .collect::<Vec<_>>();
    selection_rows
        .sort_by(|left, right| left["query_id"].as_str().cmp(&right["query_id"].as_str()));
    let mut path_rows = valid
        .values()
        .flat_map(|query| query.path_rows.clone())
        .collect::<Vec<_>>();
    path_rows.sort_by_key(path_sort_key);
    let projection_rows = valid
        .iter()
        .map(|(query_id, execution)| {
            json!({
                "candidates":execution.candidate_identities,
                "query_id":query_id,
                "run_id":run.run_id
            })
        })
        .collect();
    let mut artifacts = RunArtifacts {
        result: GraphRunExecution {
            queries: result_rows,
            run_id: run.run_id.clone(),
            status: "valid",
        },
        metrics,
        path_rows,
        projection_rows,
        selection_rows,
        valid,
    };
    apply_failures(validated, run, &mut artifacts, &observed_failures)?;
    Ok(artifacts)
}

fn apply_failures(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    artifacts: &mut RunArtifacts,
    failures: &ExecutionFailures,
) -> Result<(), String> {
    if !failures.run_is_invalid(&run.run_id) {
        return Ok(());
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
    for query_id in &invalid_queries {
        artifacts.valid.remove(query_id);
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
    artifacts.metrics = run_metrics(validated, run, &artifacts.valid, &artifacts.result.queries)?;
    Ok(())
}

fn execute_query(
    validated: &ValidatedCollection,
    database: &GraphDatabase,
    run: &RunIdentity,
    lane: &str,
    query: &Query,
    seed: &ResolvedSeed,
    fingerprint: &str,
) -> Result<ValidGraphQuery, String> {
    let graph_query = production_query(query, seed)?;
    let graph_result = database
        .graph_query(&graph_query, None)
        .map_err(|error| format!("D query '{}': graph execution: {error}", query.query_id))?;
    let mut matched_nodes = graph_result
        .matches
        .iter()
        .map(|matched| canonical_node(&matched.node_id))
        .collect::<Vec<_>>();
    matched_nodes.sort_by(|left, right| {
        canonical_json(left)
            .unwrap()
            .cmp(&canonical_json(right).unwrap())
    });
    ensure_sorted_unique(&matched_nodes, "matched nodes")?;

    let projected = database
        .project_candidates(&graph_result)
        .map_err(|error| format!("D query '{}': projection: {error}", query.query_id))?;
    let filter = query
        .metadata_filter
        .as_ref()
        .map(convert_filter)
        .transpose()?;
    let filtered = database
        .corpus()
        .filter_candidate_scope(&projected.scope, filter.as_ref())
        .map_err(|error| {
            format!(
                "D query '{}': filter projected scope: {error}",
                query.query_id
            )
        })?;
    let identities = database
        .corpus()
        .candidate_scope_identities(&filtered)
        .map_err(|error| {
            format!(
                "D query '{}': hydrate projected scope: {error}",
                query.query_id
            )
        })?;
    let candidate_documents = identities
        .iter()
        .map(|identity| identity.record_id.as_str().to_owned())
        .collect::<BTreeSet<_>>();

    let all_scope = database
        .corpus()
        .candidate_scope(
            database
                .corpus()
                .chunk_identities()
                .map(|(_, chunk_id)| chunk_id),
        )
        .map_err(|error| format!("D query '{}': active corpus scope: {error}", query.query_id))?;
    let eligible = database
        .corpus()
        .filter_candidate_scope(&all_scope, filter.as_ref())
        .map_err(|error| format!("D query '{}': corpus filter: {error}", query.query_id))?;

    let path_rows = graph_result
        .matches
        .iter()
        .map(|matched| path_row(&query.query_id, &run.run_id, matched))
        .collect::<Result<Vec<_>, _>>()?;
    let truncated_reason = truncation_name(graph_result.truncated);
    let metrics = query_metrics(QueryMetricContext {
        validated,
        lane,
        query,
        documents: &candidate_documents,
        eligible_chunks: eligible.len(),
        projected_chunks: identities.len(),
        path_rows: &path_rows,
        truncated: graph_result.truncated,
    })?;
    let selection_row = json!({
        "active_corpus_chunks_before_filter":database.corpus().active_chunk_count(),
        "corpus_id":database.corpus().corpus_id().as_str(),
        "eligible_corpus_chunks_after_filter":eligible.len(),
        "generation_fingerprint":fingerprint,
        "matched_nodes":matched_nodes,
        "projected_chunks_after_filter":identities.len(),
        "projected_chunks_before_filter":projected.trace.resolved_chunks,
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
        "truncated_reason":truncated_reason
    });
    if graph_result.trace.result_count != matched_nodes.len()
        || identities.len() > projected.trace.resolved_chunks
        || candidate_documents.len() > identities.len()
        || eligible.len() > database.corpus().active_chunk_count()
    {
        return Err(format!(
            "D query '{}': selection count invariant",
            query.query_id
        ));
    }
    Ok(ValidGraphQuery {
        candidate_documents,
        candidate_identities: identities.iter().map(canonical_identity).collect(),
        eligible_chunks: eligible.len(),
        metrics,
        path_rows,
        projected_chunks: identities.len(),
        selection_row,
    })
}

fn production_query(query: &Query, seed: &ResolvedSeed) -> Result<GraphQuery, String> {
    let mut result = GraphQuery::new(seed.production.clone());
    for step in &query.traversal.steps {
        result = result.traverse(Traverse {
            relationship: RelationshipType::new(step.relationship_type.clone())
                .map_err(|error| format!("D traversal relationship: {error}"))?,
            direction: match step.direction.as_str() {
                "outgoing" => Direction::Outgoing,
                "incoming" => Direction::Incoming,
                actual => return Err(format!("D traversal direction '{actual}' is invalid")),
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
            "excluded seed '{query_id}' unexpectedly executed: {reason}"
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

fn result_row(
    query: &Query,
    run: &RunIdentity,
    execution_status: &'static str,
    status_reason: Option<String>,
) -> GraphQueryExecution {
    GraphQueryExecution {
        candidate_limits: CandidateLimits {
            keyword: None,
            vector: None,
        },
        chunk_hits: Vec::new(),
        duplicate_collapse_count: 0,
        execution_status,
        filter: query.metadata_filter.clone(),
        projected_documents: Vec::new(),
        query_id: query.query_id.clone(),
        selection_run_id: Some(run.run_id.clone()),
        status_reason,
    }
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

fn canonical_identity(identity: &ChunkIdentity) -> Value {
    json!({
        "chunk_key":identity.chunk_key.as_str(),
        "record_id":identity.record_id.as_str()
    })
}

fn path_row(
    query_id: &str,
    run_id: &str,
    matched: &vectorkit_graph::GraphMatch,
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
            return Err("graph path is not contiguous with its matched node".to_owned());
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

fn query_metrics(
    context: QueryMetricContext<'_>,
) -> Result<BTreeMap<&'static str, Metric>, String> {
    let QueryMetricContext {
        validated,
        lane,
        query,
        documents,
        eligible_chunks,
        projected_chunks,
        path_rows,
        truncated,
    } = context;
    let mut metrics = D_METRICS
        .iter()
        .map(|name| (*name, Metric::status("not_applicable")))
        .collect::<BTreeMap<_, _>>();
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
    let reason = truncation_name(truncated);
    metrics.insert(
        "truncated",
        Metric::value(if reason.is_some() { 1.0 } else { 0.0 }),
    );
    for (metric, expected) in [
        ("truncated_max_hops", "max_hops"),
        ("truncated_max_results", "max_results"),
        ("truncated_max_visited", "max_visited"),
        ("truncated_max_working_bytes", "max_working_bytes"),
    ] {
        metrics.insert(
            metric,
            Metric::value(if reason == Some(expected) { 1.0 } else { 0.0 }),
        );
    }

    if query.tasks.iter().any(|task| task == "evidence") {
        let evidence = validated
            .evidence
            .iter()
            .find(|row| row.query_id == query.query_id)
            .ok_or_else(|| {
                format!(
                    "missing evidence for '{}': validation drift",
                    query.query_id
                )
            })?;
        let (matched, required) = best_evidence(documents, evidence)?;
        metrics.insert(
            "candidate_recall",
            Metric::value(matched as f64 / required as f64),
        );
        metrics.insert(
            "candidate_complete_evidence",
            Metric::value(if matched == required { 1.0 } else { 0.0 }),
        );
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

fn best_evidence(
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

fn path_matches(actual_rows: &[Value], expected: &ExpectedPaths) -> Result<bool, String> {
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

fn run_metrics(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    valid: &BTreeMap<String, ValidGraphQuery>,
    results: &[GraphQueryExecution],
) -> Result<Value, String> {
    let mut queries = Vec::new();
    for query_id in &run.declared {
        if let Some(execution) = valid.get(query_id) {
            queries.push(json!({
                "candidate_counts":{"eligible_chunks":execution.eligible_chunks,"projected_chunks":execution.projected_chunks},
                "execution_status":"valid",
                "metrics":metrics_json(&execution.metrics),
                "query_id":query_id
            }));
        } else if run.execution.contains(query_id) {
            let result = results
                .iter()
                .find(|result| result.query_id == *query_id)
                .ok_or_else(|| format!("missing D result row for '{query_id}'"))?;
            let metrics = D_METRICS
                .iter()
                .map(|name| (*name, Metric::status("invalid_execution")))
                .collect();
            queries.push(json!({
                "candidate_counts":Value::Null,
                "execution_status":result.execution_status,
                "metrics":metrics_json(&metrics),
                "query_id":query_id
            }));
        } else {
            let metrics = D_METRICS
                .iter()
                .map(|name| (*name, Metric::status("excluded_pre_freeze")))
                .collect();
            queries.push(json!({
                "candidate_counts":Value::Null,
                "execution_status":"excluded_pre_freeze",
                "metrics":metrics_json(&metrics),
                "query_id":query_id
            }));
        }
    }
    let macro_metrics = macro_metrics(&queries)?;
    let micro = micro_metrics(validated, run, valid)?;
    Ok(json!({
        "counts":{
            "attempted":run.execution.len(),
            "declared":run.declared.len(),
            "excluded_pre_freeze":run.declared.len()-run.execution.len(),
            "invalid_execution":run.execution.len()-valid.len(),
            "valid_execution":valid.len()
        },
        "declared_population_sha256":population_hash(&run.declared),
        "execution_population_sha256":population_hash(&run.execution),
        "macro":macro_metrics,
        "micro":micro,
        "queries":queries,
        "run_id":run.run_id,
        "status":if valid.len()==run.execution.len(){"valid"}else{"invalid_execution"}
    }))
}

fn metrics_json(metrics: &BTreeMap<&'static str, Metric>) -> Value {
    Value::Object(
        metrics
            .iter()
            .map(|(name, metric)| ((*name).to_owned(), metric.json()))
            .collect(),
    )
}

fn macro_metrics(queries: &[Value]) -> Result<Value, String> {
    let mut result = Map::new();
    for metric in D_METRICS {
        let mut numerator = 0.0;
        let mut denominator = 0_u64;
        let mut counts = BTreeMap::from([
            ("excluded_pre_freeze", 0_u64),
            ("invalid_execution", 0),
            ("not_applicable", 0),
            ("undefined", 0),
            ("valid", 0),
        ]);
        for query in queries {
            let row = &query["metrics"][metric];
            let status = row["status"]
                .as_str()
                .ok_or_else(|| format!("metric '{metric}' status missing"))?;
            *counts
                .get_mut(status)
                .ok_or_else(|| format!("metric '{metric}' status '{status}' invalid"))? += 1;
            if status == "valid" {
                numerator += row["value"]
                    .as_f64()
                    .ok_or_else(|| format!("metric '{metric}' value missing"))?;
                denominator += 1;
            }
        }
        result.insert(
            metric.to_owned(),
            json!({
                "denominator":denominator,
                "numerator":numerator,
                "status_counts":counts,
                "value":if denominator==0{Value::Null}else{json!(numerator/denominator as f64)}
            }),
        );
    }
    Ok(Value::Object(result))
}

fn micro_metrics(
    validated: &ValidatedCollection,
    run: &RunIdentity,
    valid: &BTreeMap<String, ValidGraphQuery>,
) -> Result<Value, String> {
    let mut matched = 0_usize;
    let mut required = 0_usize;
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
    for (query_id, execution) in valid {
        eligible += execution.eligible_chunks;
        projected += execution.projected_chunks;
        empty += usize::from(execution.projected_chunks == 0);
        let query = validated
            .queries
            .iter()
            .find(|query| query.query_id == *query_id)
            .unwrap();
        if query.tasks.iter().any(|task| task == "evidence") {
            let evidence = validated
                .evidence
                .iter()
                .find(|row| row.query_id == *query_id)
                .unwrap();
            let choice = best_evidence(&execution.candidate_documents, evidence)?;
            matched += choice.0;
            required += choice.1;
        }
        let reason = execution.selection_row["truncated_reason"].as_str();
        if let Some(reason) = reason {
            *truncated.get_mut("all").unwrap() += 1;
            *truncated.get_mut(reason).unwrap() += 1;
        }
    }
    let graph_valid = valid.len();
    let rate = |numerator: usize, denominator: usize| {
        if denominator == 0 {
            Value::Null
        } else {
            json!(numerator as f64 / denominator as f64)
        }
    };
    let evidence = json!({
        "matched_documents":matched,
        "required_documents":required,
        "value":rate(matched,required)
    });
    let _ = run;
    Ok(json!({
        "candidate_recall":evidence,
        "candidate_reduction_ratio":{
            "candidate_chunks":projected,
            "eligible_chunks":eligible,
            "value":rate(eligible,projected)
        },
        "empty_scope_rate":{"empty_scopes":empty,"graph_valid_queries":graph_valid,"value":rate(empty,graph_valid)},
        "supporting_document_recall_at_10":{"matched_documents":0,"required_documents":0,"value":Value::Null},
        "supporting_document_recall_at_5":{"matched_documents":0,"required_documents":0,"value":Value::Null},
        "truncation_rate":{"affected_queries":truncated["all"],"graph_valid_queries":graph_valid,"value":rate(truncated["all"],graph_valid)},
        "truncation_rate_max_hops":{"affected_queries":truncated["max_hops"],"graph_valid_queries":graph_valid,"value":rate(truncated["max_hops"],graph_valid)},
        "truncation_rate_max_results":{"affected_queries":truncated["max_results"],"graph_valid_queries":graph_valid,"value":rate(truncated["max_results"],graph_valid)},
        "truncation_rate_max_visited":{"affected_queries":truncated["max_visited"],"graph_valid_queries":graph_valid,"value":rate(truncated["max_visited"],graph_valid)},
        "truncation_rate_max_working_bytes":{"affected_queries":truncated["max_working_bytes"],"graph_valid_queries":graph_valid,"value":rate(truncated["max_working_bytes"],graph_valid)}
    }))
}

fn ensure_sorted_unique(values: &[Value], label: &str) -> Result<(), String> {
    let encoded = values
        .iter()
        .map(canonical_json)
        .collect::<Result<Vec<_>, _>>()?;
    if encoded.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(format!("{label} are not duplicate-free canonical order"));
    }
    Ok(())
}

fn path_sort_key(value: &Value) -> (String, String, String) {
    (
        value["query_id"].as_str().unwrap().to_owned(),
        canonical_json(&value["matched_node"]).unwrap(),
        canonical_json(&value["edges"]).unwrap(),
    )
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
                "failed to create graph persistence directory '{}': {error}",
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

    use vectorkit_core::{CorpusId, RecordInput, VectorKitError};
    use vectorkit_graph::GraphError;

    use super::*;
    use crate::quality::v3_graph_input::production_schema;
    use crate::quality::v3_ingestion::V3ProductionInputs;
    use crate::quality::v3_validation::validate;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    #[test]
    fn executes_all_frozen_d_lanes_through_graph_only_databases() {
        let validated = validate(&fixture_root()).unwrap();
        let seeds = resolve_seeds(&validated).unwrap();
        validate_frozen_runs(&validated).unwrap();
        let (_, fingerprint) = d_generation_fingerprint(&validated).unwrap();
        let runs = validated
            .runs
            .iter()
            .filter(|run| run.configuration["run_letter"] == "d")
            .map(|run| {
                let database = build_graph_database(&validated).unwrap();
                assert_eq!(database.graph().node_count(), 15);
                assert_eq!(database.graph().edge_count(), 26);
                assert_eq!(database.corpus().active_chunk_count(), 8);
                execute_run(&validated, run, &database, &seeds, &fingerprint).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(runs.len(), 3);
        assert_eq!(
            runs.iter()
                .map(|run| run.selection_rows.len())
                .sum::<usize>(),
            7
        );
        assert_eq!(
            runs.iter().map(|run| run.path_rows.len()).sum::<usize>(),
            14
        );
        assert!(runs.iter().all(|run| run.result.status == "valid"));
    }

    #[test]
    fn graph_generation_fingerprint_is_stable_and_retrieval_free() {
        let validated = validate(&fixture_root()).unwrap();
        let (preimage, fingerprint) = d_generation_fingerprint(&validated).unwrap();
        assert!(preimage["retrieval_state_sha256"].is_null());
        assert_eq!(fingerprint.len(), 64);
        assert_eq!(
            d_generation_fingerprint(&validated).unwrap(),
            (preimage, fingerprint)
        );
    }

    #[test]
    fn persisted_d_lanes_reexecute_to_identical_stable_artifacts() {
        let validated = validate(&fixture_root()).unwrap();
        let seeds = resolve_seeds(&validated).unwrap();
        let (_, fingerprint) = d_generation_fingerprint(&validated).unwrap();
        for run in validated
            .runs
            .iter()
            .filter(|run| run.configuration["run_letter"] == "d")
        {
            let database = build_graph_database(&validated).unwrap();
            let (artifacts, persistence) =
                execute_run_with_persistence(&validated, run, &database, &seeds, &fingerprint)
                    .unwrap();
            assert_eq!(artifacts.result.status, "valid");
            assert!(persistence.save_validate_load_equivalent);
            assert_eq!(persistence.stable_generation_fingerprint, fingerprint);
        }
    }

    #[test]
    fn rejects_cross_corpus_cross_generation_and_stale_candidate_operations() {
        let validated = validate(&fixture_root()).unwrap();
        let seeds = resolve_seeds(&validated).unwrap();
        let query = validated
            .queries
            .iter()
            .find(|query| query.query_id == "qc")
            .unwrap();
        let seed = resolved_seed(&seeds, "explicit", "qc").unwrap();
        let graph_query = production_query(query, seed).unwrap();
        let database = build_graph_database(&validated).unwrap();
        let result = database.graph_query(&graph_query, None).unwrap();
        let projected = database.project_candidates(&result).unwrap();

        let mut inputs = V3ProductionInputs::from_validated(&validated).unwrap();
        inputs.corpus_id = CorpusId::new("v3-cross-corpus").unwrap();
        let cross_corpus = GraphDatabase::build(
            inputs.build_corpus().unwrap(),
            production_schema(&validated.graph_schema).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            cross_corpus.project_candidates(&result),
            Err(GraphError::StaleGeneration { .. })
        ));

        let inputs = V3ProductionInputs::from_validated(&validated).unwrap();
        let mut newer_corpus = inputs.build_corpus().unwrap();
        let replacement = &inputs.records[0];
        newer_corpus
            .upsert(RecordInput {
                record: replacement.record.clone(),
                metadata: replacement.inherited_metadata.clone(),
                chunks: replacement.chunks.clone(),
            })
            .unwrap();
        assert!(matches!(
            newer_corpus.filter_candidate_scope(&projected.scope, None),
            Err(VectorKitError::StaleGeneration { .. })
        ));
        assert!(matches!(
            newer_corpus.candidate_scope_identities(&projected.scope),
            Err(VectorKitError::StaleGeneration { .. })
        ));
        let newer_database = GraphDatabase::build(
            newer_corpus,
            production_schema(&validated.graph_schema).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            newer_database.project_candidates(&result),
            Err(GraphError::StaleGeneration { .. })
        ));
    }
}
