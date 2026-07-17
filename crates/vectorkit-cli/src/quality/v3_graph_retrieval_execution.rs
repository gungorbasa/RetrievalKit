use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};
use vectorkit_core::{HybridHit, HybridQuery, SearchQuery, VectorEncoding};
use vectorkit_graph::{
    Direction, GraphPathEdge, GraphQuery, GraphRetrievalDatabase, NodeId, NodeSource, QueryLimits,
    RelationshipType, Traverse, TruncationReason,
};

use super::v3::retrieval_generation_fingerprint;
use super::v3_canonical::{canonical_json, canonical_json_line, write_canonical_json};
use super::v3_graph_input::build_graph_retrieval_database;
use super::v3_ingestion::{convert_filter, ProductionQueryInput, V3ProductionInputs};
use super::v3_runs::RunIdentity;
use super::v3_schema::Query;
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct GraphRetrievalQualificationResults {
    collection_id: String,
    collection_version: String,
    runs: Vec<RunExecution>,
    schema_version: u8,
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

#[derive(Debug, Clone, PartialEq)]
struct ValidQuery {
    result: QueryExecution,
    path_rows: Vec<Value>,
    projection_row: Value,
    selection_row: Value,
}

pub(super) fn emit_graph_retrieval_qualification(
    validated: &ValidatedCollection,
    output: &Path,
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
        runs.push(execute_run(
            validated,
            run,
            &database,
            &inputs,
            &source_queries,
            &seeds,
            &fingerprint,
        )?);
    }
    runs.sort_by(|left, right| left.result.run_id.cmp(&right.result.run_id));
    if runs.len() != 9 {
        return Err(format!(
            "V3 Phase 1.2c expected nine E-G runs, actual {}",
            runs.len()
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
    let equality = validate_selection_path_equality_with_d(output, &runs)?;
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
    Ok(results)
}

fn validate_selection_path_equality_with_d(
    output: &Path,
    runs: &[RunArtifacts],
) -> Result<Value, String> {
    let d_run_id = |lane: &str| match lane {
        "explicit" => Ok("v3-d-selection-none-none-explicit-cfg-13feb2a18ac3"),
        "topic" => Ok("v3-d-selection-none-none-topic-cfg-bf6bed5c72e7"),
        "team" => Ok("v3-d-selection-none-none-team-cfg-7278e2315c8f"),
        actual => Err(format!("unsupported graph retrieval lane '{actual}'")),
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
        let run_identity = run
            .selection_rows
            .first()
            .ok_or_else(|| format!("run '{}' has no valid selections", run.result.run_id))?;
        let lane = run_identity["seed_lane"]
            .as_str()
            .ok_or_else(|| format!("run '{}' selection has no lane", run.result.run_id))?;
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
        if actual_selections != expected_selections || actual_paths != expected_paths {
            return Err(format!(
                "run '{}' selection/path differs logically from D lane '{}'",
                run.result.run_id, lane
            ));
        }
        rows.push(json!({
            "d_run_id":d_run_id,
            "path_rows":actual_paths.len(),
            "query_count":actual_selections.len(),
            "run_id":run.result.run_id,
            "selection_equal":true,
            "path_equal":true
        }));
    }
    rows.sort_by_key(|row| row["run_id"].as_str().unwrap().to_owned());
    Ok(json!({"runs":rows,"schema_version":1,"status":"valid"}))
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
    for (run_id, logical, lane) in SEMANTIC_RUNS {
        let run = validated
            .runs
            .iter()
            .find(|run| run.run_id == run_id)
            .ok_or_else(|| format!("missing frozen semantic run '{run_id}'"))?;
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
    for (run_id, logical, lane) in HYBRID_RUNS {
        let run = validated
            .runs
            .iter()
            .find(|run| run.run_id == run_id)
            .ok_or_else(|| format!("missing frozen hybrid run '{run_id}'"))?;
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
        let execution = execute_query(
            validated,
            database,
            run,
            lane,
            source,
            input,
            seed,
            fingerprint,
        )?;
        result_rows.push(execution.result.clone());
        valid.push(execution);
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
    Ok(RunArtifacts {
        result: RunExecution {
            queries: result_rows,
            run_id: run.run_id.clone(),
            status: "valid",
        },
        path_rows,
        projection_rows,
        selection_rows,
    })
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
    graph_result: &vectorkit_graph::GraphResult,
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
    graph_result: &vectorkit_graph::GraphResult,
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
    database
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
        .collect()
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

fn canonical_identity(identity: &vectorkit_core::ChunkIdentity) -> Value {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::quality::v3_validation::validate;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
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
}
