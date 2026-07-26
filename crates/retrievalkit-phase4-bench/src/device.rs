use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

use retrievalkit_core::{
    CandidateScope, Filter, HybridQuery, KeywordQuery, SearchQuery, VectorEncoding,
};
use retrievalkit_graph::{GraphResult, GraphRetrievalDatabase};
use serde::{Deserialize, Serialize};

use super::{
    build_retrieval_database, expect_top_identity, next_hop_graph_query, phase4_graph_schema,
    sha256_hex, source_embedding, validate_database_behavior, validate_database_shape,
    WorkloadSpec, QUERY_CATEGORIES, TOP_K,
};

const WARMUPS: usize = 100;
const SAMPLES: usize = 1_000;
const STAGES: [&str; 7] = [
    "seed_resolution",
    "traversal",
    "projection",
    "filter_intersection",
    "ranking",
    "hydration",
    "end_to_end_total",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceQueryConfig {
    workload_id: String,
    encoding: String,
    session_id: String,
}

#[derive(Debug, Serialize)]
struct DeviceQuerySession {
    schema_version: u32,
    artifact_type: &'static str,
    workload_id: String,
    classification: String,
    encoding: String,
    session_id: String,
    build_configuration: &'static str,
    embedding_included: bool,
    warmups_per_scenario: usize,
    samples_per_scenario: usize,
    percentile_method: &'static str,
    raw_unit: &'static str,
    stages: Vec<&'static str>,
    query_categories: Vec<&'static str>,
    build_ns: u64,
    correctness_checks: Vec<String>,
    scenarios: Vec<ScenarioReport>,
    supported_v1_capacity_changed: bool,
}

#[derive(Debug, Serialize)]
struct ScenarioReport {
    query_category: &'static str,
    result_identity_sha256: String,
    selection_identity_sha256: String,
    path_identity_sha256: String,
    filter_identity_sha256: String,
    distributions: Vec<Distribution>,
    samples: Vec<QuerySample>,
}

#[derive(Debug, Serialize)]
struct QuerySample {
    sample_index: usize,
    stages: Vec<StageSample>,
    result_identity_sha256: String,
    selection_identity_sha256: String,
    path_identity_sha256: String,
    filter_identity_sha256: String,
    deleted_results: usize,
}

#[derive(Debug, Serialize)]
struct StageSample {
    stage: &'static str,
    sequence: usize,
    duration_ns: u64,
    directly_measured: bool,
}

#[derive(Debug, Serialize)]
struct Distribution {
    stage: &'static str,
    sample_count: usize,
    min_ns: u64,
    max_ns: u64,
    mean_ns: u64,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
}

struct OneMeasurement {
    durations: [u64; 7],
    result_identity_sha256: String,
    selection_identity_sha256: String,
    path_identity_sha256: String,
    filter_identity_sha256: String,
    deleted_results: usize,
}

pub fn run_device_query_session_json(config_json: &str) -> Result<String, String> {
    if cfg!(debug_assertions) {
        return Err("Phase 4b device sessions require an optimized release build".to_owned());
    }
    let config: DeviceQueryConfig = serde_json::from_str(config_json)
        .map_err(|error| format!("invalid device config: {error}"))?;
    if config.session_id.trim().is_empty() {
        return Err("device session_id cannot be empty".to_owned());
    }
    let spec = WorkloadSpec::parse(&config.workload_id)?;
    spec.validate()?;
    let encoding = match config.encoding.as_str() {
        "f32" => VectorEncoding::F32,
        "i8" => VectorEncoding::I8ScalarQuantized,
        value => return Err(format!("unsupported Phase 4b encoding '{value}'")),
    };

    let build_started = Instant::now();
    let retrieval = build_retrieval_database(spec, encoding)?;
    let database = GraphRetrievalDatabase::build(retrieval, phase4_graph_schema()?)
        .map_err(|error| format!("failed to build Phase 4b graph database: {error}"))?;
    let build_ns = elapsed_ns(build_started);
    validate_database_shape(&database, spec, encoding)?;
    let correctness_checks = validate_database_behavior(&database, spec)?;
    let all_candidates = database
        .corpus()
        .candidate_scope(0..spec.active_chunks as u64)
        .map_err(|error| format!("failed to create complete active candidate scope: {error}"))?;

    let scenarios = QUERY_CATEGORIES
        .iter()
        .map(|category| run_scenario(&database, spec, &all_candidates, category))
        .collect::<Result<Vec<_>, _>>()?;
    let report = DeviceQuerySession {
        schema_version: 1,
        artifact_type: "phase4b_device_query_session",
        workload_id: spec.id.to_owned(),
        classification: spec.class.label().to_owned(),
        encoding: config.encoding,
        session_id: config.session_id,
        build_configuration: "release",
        embedding_included: false,
        warmups_per_scenario: WARMUPS,
        samples_per_scenario: SAMPLES,
        percentile_method: "nearest_rank",
        raw_unit: "integer_nanoseconds",
        stages: STAGES.to_vec(),
        query_categories: QUERY_CATEGORIES.to_vec(),
        build_ns,
        correctness_checks,
        scenarios,
        supported_v1_capacity_changed: false,
    };
    serde_json::to_string(&report)
        .map_err(|error| format!("failed to encode device report: {error}"))
}

fn run_scenario(
    database: &GraphRetrievalDatabase,
    spec: WorkloadSpec,
    all_candidates: &CandidateScope,
    category: &'static str,
) -> Result<ScenarioReport, String> {
    for _ in 0..WARMUPS {
        black_box(measure_once(database, spec, all_candidates, category)?);
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    let mut stage_values = STAGES
        .iter()
        .map(|stage| (*stage, Vec::with_capacity(SAMPLES)))
        .collect::<BTreeMap<_, _>>();
    let mut expected = None;
    for sample_index in 0..SAMPLES {
        let measured = measure_once(database, spec, all_candidates, category)?;
        let identities = (
            measured.result_identity_sha256.clone(),
            measured.selection_identity_sha256.clone(),
            measured.path_identity_sha256.clone(),
            measured.filter_identity_sha256.clone(),
        );
        if expected.as_ref().is_some_and(|value| value != &identities) {
            return Err(format!(
                "{category} identities changed at sample {sample_index}"
            ));
        }
        expected.get_or_insert_with(|| identities.clone());
        let stages = STAGES
            .iter()
            .enumerate()
            .map(|(sequence, stage)| {
                let duration_ns = measured.durations[sequence];
                stage_values
                    .get_mut(stage)
                    .expect("declared stage")
                    .push(duration_ns);
                StageSample {
                    stage,
                    sequence,
                    duration_ns,
                    directly_measured: *stage == "end_to_end_total",
                }
            })
            .collect();
        samples.push(QuerySample {
            sample_index,
            stages,
            result_identity_sha256: identities.0,
            selection_identity_sha256: identities.1,
            path_identity_sha256: identities.2,
            filter_identity_sha256: identities.3,
            deleted_results: measured.deleted_results,
        });
    }
    let identities = expected.ok_or_else(|| format!("{category} produced no samples"))?;
    let distributions = STAGES
        .iter()
        .map(|stage| distribution(stage, &stage_values[stage]))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ScenarioReport {
        query_category: category,
        result_identity_sha256: identities.0,
        selection_identity_sha256: identities.1,
        path_identity_sha256: identities.2,
        filter_identity_sha256: identities.3,
        distributions,
        samples,
    })
}

fn measure_once(
    database: &GraphRetrievalDatabase,
    spec: WorkloadSpec,
    all_candidates: &CandidateScope,
    category: &str,
) -> Result<OneMeasurement, String> {
    let target_record = spec.active_records() / 3;
    let mut durations = [0; 7];
    let mut selection: Option<GraphResult> = None;
    let mut filter_identity = String::new();
    let total_started = Instant::now();

    let hit_ids: Vec<u64> = match category {
        "semantic" => {
            let started = Instant::now();
            let hits = database
                .semantic_search(&SearchQuery::new(
                    source_embedding(target_record, 0, false),
                    TOP_K,
                ))
                .map_err(|error| format!("semantic measurement failed: {error}"))?;
            durations[4] = elapsed_ns(started);
            expect_top_identity(
                database,
                hits.first().map(|hit| hit.chunk_id),
                target_record,
                0,
            )?;
            hits.into_iter().map(|hit| hit.chunk_id).collect()
        }
        "exact_name" => {
            let started = Instant::now();
            let hits = database
                .retrieval()
                .as_compatibility_index()
                .keyword_search(&KeywordQuery::new(
                    format!("identity{target_record:08}"),
                    TOP_K,
                ))
                .map_err(|error| format!("exact-name measurement failed: {error}"))?;
            durations[4] = elapsed_ns(started);
            expect_top_identity(
                database,
                hits.first().map(|hit| hit.chunk_id),
                target_record,
                0,
            )?;
            hits.into_iter().map(|hit| hit.chunk_id).collect()
        }
        "hybrid" => {
            let started = Instant::now();
            let hits = database
                .hybrid_search(&HybridQuery::new(
                    format!("identity{target_record:08}"),
                    source_embedding(target_record, 0, false),
                    TOP_K,
                ))
                .map_err(|error| format!("hybrid measurement failed: {error}"))?;
            durations[4] = elapsed_ns(started);
            expect_top_identity(
                database,
                hits.first().map(|hit| hit.chunk_id),
                target_record,
                0,
            )?;
            hits.into_iter().map(|hit| hit.chunk_id).collect()
        }
        "metadata_filter" => {
            let filter = Filter::eq("tenant", format!("tenant-{}", target_record % 4));
            filter_identity = format!("tenant=tenant-{}", target_record % 4);
            let started = Instant::now();
            let filtered = database
                .corpus()
                .filter_candidate_scope(all_candidates, Some(&filter))
                .map_err(|error| format!("metadata filter measurement failed: {error}"))?;
            durations[3] = elapsed_ns(started);
            let started = Instant::now();
            let hits = database
                .retrieval()
                .semantic_search_in_candidates(
                    &SearchQuery::new(source_embedding(target_record, 0, false), TOP_K),
                    &filtered,
                )
                .map_err(|error| format!("filtered ranking measurement failed: {error}"))?;
            durations[4] = elapsed_ns(started);
            expect_top_identity(
                database,
                hits.first().map(|hit| hit.chunk_id),
                target_record,
                0,
            )?;
            hits.into_iter().map(|hit| hit.chunk_id).collect()
        }
        "graph_1hop" | "graph_2hop" | "graph_3hop" | "graph_filter" => {
            let hops = match category {
                "graph_1hop" => 1,
                "graph_2hop" => 2,
                "graph_3hop" => 3,
                _ => 4,
            };
            let query = next_hop_graph_query(0, hops)?;
            let (graph_result, timings) = database
                .graph_query_with_timings(&query, None)
                .map_err(|error| format!("{category} traversal measurement failed: {error}"))?;
            durations[0] = timings.seed_resolution_ns;
            durations[1] = timings.traversal_ns;
            let started = Instant::now();
            let projected = database
                .project_candidates(&graph_result)
                .map_err(|error| format!("{category} projection measurement failed: {error}"))?;
            durations[2] = elapsed_ns(started);
            let scope = if category == "graph_filter" {
                let filter = Filter::eq("tenant", format!("tenant-{}", hops % 4));
                filter_identity = format!("tenant=tenant-{}", hops % 4);
                let started = Instant::now();
                let filtered = database
                    .corpus()
                    .filter_candidate_scope(&projected.scope, Some(&filter))
                    .map_err(|error| format!("graph/filter intersection failed: {error}"))?;
                durations[3] = elapsed_ns(started);
                filtered
            } else {
                projected.scope
            };
            let started = Instant::now();
            let hits = database
                .retrieval()
                .semantic_search_in_candidates(
                    &SearchQuery::new(source_embedding(hops, 0, false), TOP_K),
                    &scope,
                )
                .map_err(|error| format!("{category} ranking measurement failed: {error}"))?;
            durations[4] = elapsed_ns(started);
            expect_top_identity(database, hits.first().map(|hit| hit.chunk_id), hops, 0)?;
            selection = Some(graph_result);
            hits.into_iter().map(|hit| hit.chunk_id).collect()
        }
        value => return Err(format!("unknown Phase 4b query category '{value}'")),
    };

    let hydration_started = Instant::now();
    let hydrated = database.corpus().hydrate_chunks(&hit_ids);
    if hydrated.iter().any(Option::is_none) {
        return Err(format!(
            "{category} hydration returned a deleted or missing chunk"
        ));
    }
    black_box(&hydrated);
    durations[5] = elapsed_ns(hydration_started);
    durations[6] = elapsed_ns(total_started);

    let result_identities = hit_ids
        .iter()
        .map(|chunk_id| {
            database
                .corpus()
                .chunk_identity(*chunk_id)
                .map(|identity| {
                    format!(
                        "{}:{}",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    )
                })
                .ok_or_else(|| "ranked chunk has no stable identity".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (selection_hash, path_hash) = if let Some(selection) = selection {
        let selection_json = serde_json::to_vec(&selection.matches)
            .map_err(|error| format!("failed to encode selection: {error}"))?;
        let paths = selection
            .matches
            .iter()
            .flat_map(|item| item.path.iter())
            .collect::<Vec<_>>();
        let path_json = serde_json::to_vec(&paths)
            .map_err(|error| format!("failed to encode paths: {error}"))?;
        (sha256_hex(&selection_json), sha256_hex(&path_json))
    } else {
        (sha256_hex(b""), sha256_hex(b""))
    };
    let deleted_results = result_identities
        .iter()
        .filter(|identity| identity.starts_with("deleted-"))
        .count();
    Ok(OneMeasurement {
        durations,
        result_identity_sha256: sha256_hex(result_identities.join("\n").as_bytes()),
        selection_identity_sha256: selection_hash,
        path_identity_sha256: path_hash,
        filter_identity_sha256: sha256_hex(filter_identity.as_bytes()),
        deleted_results,
    })
}

fn distribution(stage: &'static str, values: &[u64]) -> Result<Distribution, String> {
    if values.len() != SAMPLES {
        return Err(format!(
            "stage {stage} has {} samples, expected {SAMPLES}",
            values.len()
        ));
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let sum = sorted.iter().map(|value| u128::from(*value)).sum::<u128>();
    Ok(Distribution {
        stage,
        sample_count: sorted.len(),
        min_ns: sorted[0],
        max_ns: sorted[sorted.len() - 1],
        mean_ns: u64::try_from(sum / sorted.len() as u128).unwrap_or(u64::MAX),
        p50_ns: nearest_rank(&sorted, 50),
        p95_ns: nearest_rank(&sorted, 95),
        p99_ns: nearest_rank(&sorted, 99),
    })
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100).max(1);
    sorted[rank - 1]
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_config_rejects_unknown_fields_and_encodings() {
        let unknown =
            r#"{"workload_id":"10k-384d-v3","encoding":"f32","session_id":"s","extra":1}"#;
        assert!(serde_json::from_str::<DeviceQueryConfig>(unknown).is_err());
        assert_eq!(nearest_rank(&(1..=100).collect::<Vec<_>>(), 95), 95);
    }
}
