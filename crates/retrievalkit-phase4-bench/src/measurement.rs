use std::collections::BTreeMap;
use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use retrievalkit_core::{Filter, SearchQuery};
use retrievalkit_graph::GraphRetrievalDatabase;
use serde::{Deserialize, Serialize};

use super::{
    sha256_hex, source_embedding, stable_identity, WorkloadSpec, CHUNKS_PER_RECORD, TOP_K,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StagedMeasurementReport {
    schema_version: u32,
    artifact_type: String,
    phase: String,
    workload_id: String,
    classification: String,
    build_configuration: String,
    embedding_included: bool,
    warmups: usize,
    samples_per_stage: usize,
    percentile_method: String,
    raw_unit: String,
    stages: Vec<String>,
    configurations: Vec<ConfigurationMeasurement>,
    physical_device_execution: bool,
    supported_v1_capacity_changed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigurationMeasurement {
    encoding: String,
    input_database: String,
    result_identity_sha256: String,
    selection_identity_sha256: String,
    path_identity_sha256: String,
    filter_identity_sha256: String,
    distributions: Vec<Distribution>,
    samples: Vec<QuerySample>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuerySample {
    sample_index: usize,
    query_id: String,
    stages: Vec<StageSample>,
    result_identity_sha256: String,
    selection_identity_sha256: String,
    path_identity_sha256: String,
    filter_identity_sha256: String,
    deleted_results: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageSample {
    stage: String,
    sequence: usize,
    duration_ns: u64,
    directly_measured: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Distribution {
    stage: String,
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

pub(super) fn run(
    spec: WorkloadSpec,
    input_root: &Path,
    output: &Path,
) -> Result<StagedMeasurementReport, String> {
    spec.validate()?;
    if cfg!(debug_assertions) {
        return Err("staged Phase 4 measurements require an optimized release build".to_owned());
    }
    fs::create_dir_all(output)
        .map_err(|error| format!("failed to create '{}': {error}", output.display()))?;

    let mut configurations = Vec::new();
    for encoding in ["f32", "i8"] {
        let database_path = input_root.join(encoding);
        GraphRetrievalDatabase::validate_dir(&database_path).map_err(|error| {
            format!(
                "failed pre-measurement validation for '{}': {error}",
                database_path.display()
            )
        })?;
        let database = GraphRetrievalDatabase::load_from_dir(&database_path)
            .map_err(|error| format!("failed to load '{}': {error}", database_path.display()))?;
        super::validate_database_shape(
            &database,
            spec,
            if encoding == "f32" {
                retrievalkit_core::VectorEncoding::F32
            } else {
                retrievalkit_core::VectorEncoding::I8ScalarQuantized
            },
        )?;

        for _ in 0..WARMUPS {
            black_box(measure_once(&database, spec)?);
        }

        let mut samples = Vec::with_capacity(SAMPLES);
        let mut values = STAGES
            .iter()
            .map(|stage| ((*stage).to_owned(), Vec::with_capacity(SAMPLES)))
            .collect::<BTreeMap<_, _>>();
        let mut expected_identities = None;
        for sample_index in 0..SAMPLES {
            let measured = measure_once(&database, spec)?;
            let identities = (
                measured.result_identity_sha256.clone(),
                measured.selection_identity_sha256.clone(),
                measured.path_identity_sha256.clone(),
                measured.filter_identity_sha256.clone(),
            );
            if expected_identities
                .as_ref()
                .is_some_and(|expected| expected != &identities)
            {
                return Err(format!(
                    "{encoding} staged measurement identities changed at sample {sample_index}"
                ));
            }
            expected_identities.get_or_insert_with(|| identities.clone());
            let stages = STAGES
                .iter()
                .enumerate()
                .map(|(sequence, stage)| {
                    let duration_ns = measured.durations[sequence];
                    values
                        .get_mut(*stage)
                        .expect("declared stage must exist")
                        .push(duration_ns);
                    StageSample {
                        stage: (*stage).to_owned(),
                        sequence,
                        duration_ns,
                        directly_measured: *stage == "end_to_end_total",
                    }
                })
                .collect();
            samples.push(QuerySample {
                sample_index,
                query_id: "graph_filter_semantic".to_owned(),
                stages,
                result_identity_sha256: identities.0,
                selection_identity_sha256: identities.1,
                path_identity_sha256: identities.2,
                filter_identity_sha256: identities.3,
                deleted_results: measured.deleted_results,
            });
        }
        let identities = expected_identities.ok_or_else(|| "no measured samples".to_owned())?;
        let distributions = STAGES
            .iter()
            .map(|stage| distribution(stage, &values[*stage]))
            .collect::<Result<Vec<_>, _>>()?;
        configurations.push(ConfigurationMeasurement {
            encoding: encoding.to_owned(),
            input_database: database_path.display().to_string(),
            result_identity_sha256: identities.0,
            selection_identity_sha256: identities.1,
            path_identity_sha256: identities.2,
            filter_identity_sha256: identities.3,
            distributions,
            samples,
        });
    }

    let report = StagedMeasurementReport {
        schema_version: 1,
        artifact_type: "phase4a_staged_measurement".to_owned(),
        phase: "phase4a".to_owned(),
        workload_id: spec.id.to_owned(),
        classification: spec.class.label().to_owned(),
        build_configuration: "release".to_owned(),
        embedding_included: false,
        warmups: WARMUPS,
        samples_per_stage: SAMPLES,
        percentile_method: "nearest_rank".to_owned(),
        raw_unit: "integer_nanoseconds".to_owned(),
        stages: STAGES.iter().map(|value| (*value).to_owned()).collect(),
        configurations,
        physical_device_execution: false,
        supported_v1_capacity_changed: false,
    };
    let report_path = output.join("staged-measurement-report.json");
    let mut bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to encode staged report: {error}"))?;
    bytes.push(b'\n');
    fs::write(&report_path, bytes)
        .map_err(|error| format!("failed to write '{}': {error}", report_path.display()))?;
    Ok(report)
}

fn measure_once(
    database: &GraphRetrievalDatabase,
    _spec: WorkloadSpec,
) -> Result<OneMeasurement, String> {
    let filter = Filter::eq("tenant", "tenant-0");
    let filter_identity_sha256 = sha256_hex(b"tenant=tenant-0");
    let query = super::next_hop_graph_query(0, 4)?;

    let total_started = Instant::now();
    let (selection, graph_timings) = database
        .graph_query_with_timings(&query, None)
        .map_err(|error| format!("staged graph query failed: {error}"))?;

    let started = Instant::now();
    let projected = database
        .project_candidates(&selection)
        .map_err(|error| format!("staged projection failed: {error}"))?;
    let projection_ns = elapsed_ns(started);

    let started = Instant::now();
    let filtered = database
        .corpus()
        .filter_candidate_scope(&projected.scope, Some(&filter))
        .map_err(|error| format!("staged filter intersection failed: {error}"))?;
    let filter_ns = elapsed_ns(started);

    let started = Instant::now();
    let hits = database
        .retrieval()
        .semantic_search_in_candidates(
            &SearchQuery::new(source_embedding(4, 0, false), TOP_K),
            &filtered,
        )
        .map_err(|error| format!("staged ranking failed: {error}"))?;
    let ranking_ns = elapsed_ns(started);

    let hit_ids = hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>();
    let started = Instant::now();
    let hydrated = database.corpus().hydrate_chunks(&hit_ids);
    if hydrated.iter().any(Option::is_none) {
        return Err("staged hydration returned a missing/deleted chunk".to_owned());
    }
    black_box(&hydrated);
    let hydration_ns = elapsed_ns(started);
    let total_ns = elapsed_ns(total_started);

    let expected = stable_identity(4, 0)?;
    let actual = hits
        .first()
        .and_then(|hit| database.corpus().chunk_identity(hit.chunk_id))
        .ok_or_else(|| "staged ranking returned no stable top identity".to_owned())?;
    if actual != &expected || filtered.len() != CHUNKS_PER_RECORD {
        return Err("staged result/filter identity mismatch".to_owned());
    }
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
    let selection_json = serde_json::to_vec(&selection.matches)
        .map_err(|error| format!("failed to encode selection identity: {error}"))?;
    let paths = selection
        .matches
        .iter()
        .flat_map(|item| item.path.iter())
        .collect::<Vec<_>>();
    let path_json = serde_json::to_vec(&paths)
        .map_err(|error| format!("failed to encode path identity: {error}"))?;
    let deleted_results = result_identities
        .iter()
        .filter(|identity| identity.starts_with("deleted-"))
        .count();
    Ok(OneMeasurement {
        durations: [
            graph_timings.seed_resolution_ns,
            graph_timings.traversal_ns,
            projection_ns,
            filter_ns,
            ranking_ns,
            hydration_ns,
            total_ns,
        ],
        result_identity_sha256: sha256_hex(result_identities.join("\n").as_bytes()),
        selection_identity_sha256: sha256_hex(&selection_json),
        path_identity_sha256: sha256_hex(&path_json),
        filter_identity_sha256,
        deleted_results,
    })
}

fn distribution(stage: &str, values: &[u64]) -> Result<Distribution, String> {
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
        stage: stage.to_owned(),
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
    fn nearest_rank_uses_ceil_and_one_based_rank() {
        let values = (1..=100).collect::<Vec<_>>();
        assert_eq!(nearest_rank(&values, 50), 50);
        assert_eq!(nearest_rank(&values, 95), 95);
        assert_eq!(nearest_rank(&values, 99), 99);
    }

    #[test]
    fn protocol_constants_match_frozen_configuration() {
        assert_eq!(WARMUPS, 100);
        assert_eq!(SAMPLES, 1_000);
        assert_eq!(STAGES.last(), Some(&"end_to_end_total"));
    }
}
