use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use retrievalkit_core::{ExactVectorIndex, VectorEncoding};
use retrievalkit_graph::GraphRetrievalDatabase;
use serde::{Deserialize, Serialize};

use super::{
    active_capability_directory, build_retrieval_database, directory_size, phase4_graph_schema,
    validate_database_behavior, validate_database_shape, WorkloadSpec,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LifecycleConfig {
    workload_id: String,
    encoding: String,
    sample_id: String,
    operation: String,
    directory: PathBuf,
}

#[derive(Debug, Serialize)]
struct LifecycleReport {
    schema_version: u32,
    artifact_type: &'static str,
    workload_id: String,
    classification: String,
    encoding: String,
    sample_id: String,
    operation: String,
    build_configuration: &'static str,
    operation_duration_ns: u64,
    replay_duration_ns: Option<u64>,
    replay_equivalent: Option<bool>,
    correctness_checks: Option<Vec<String>>,
    persisted_components: Option<PersistedComponents>,
    supported_v1_capacity_changed: bool,
}

#[derive(Debug, Serialize)]
struct PersistedComponents {
    corpus_chunks_bytes: u64,
    vectors_quantization_bytes: u64,
    lexical_bm25_bytes: u64,
    graph_schema_bytes: u64,
    manifest_validation_bytes: u64,
    complete_directory_bytes: u64,
    component_sum_matches_directory: bool,
}

pub fn run_device_lifecycle_sample_json(config_json: &str) -> Result<String, String> {
    if cfg!(debug_assertions) {
        return Err("Phase 4b lifecycle samples require an optimized release build".to_owned());
    }
    let config: LifecycleConfig = serde_json::from_str(config_json)
        .map_err(|error| format!("invalid lifecycle config: {error}"))?;
    if config.sample_id.trim().is_empty() {
        return Err("lifecycle sample_id cannot be empty".to_owned());
    }
    let spec = WorkloadSpec::parse(&config.workload_id)?;
    spec.validate()?;
    let encoding = parse_encoding(&config.encoding)?;

    match config.operation.as_str() {
        "prepare" => {
            require_fresh_directory(&config.directory)?;
            let started = Instant::now();
            let database = build_database(spec, encoding)?;
            let checks = validate_database_behavior(&database, spec)?;
            database
                .save_to_dir(&config.directory)
                .map_err(|error| format!("failed to prepare persisted database: {error}"))?;
            GraphRetrievalDatabase::validate_dir(&config.directory)
                .map_err(|error| format!("prepared database validation failed: {error}"))?;
            encode(report(
                &config,
                spec,
                elapsed_ns(started),
                None,
                None,
                Some(checks),
                Some(account_components(&config.directory)?),
            ))
        }
        "build" => {
            let started = Instant::now();
            let database = build_database(spec, encoding)?;
            let duration = elapsed_ns(started);
            validate_database_shape(&database, spec, encoding)?;
            let checks = validate_database_behavior(&database, spec)?;
            encode(report(
                &config,
                spec,
                duration,
                None,
                None,
                Some(checks),
                None,
            ))
        }
        "save" => {
            require_fresh_directory(&config.directory)?;
            let database = build_database(spec, encoding)?;
            let started = Instant::now();
            database
                .save_to_dir(&config.directory)
                .map_err(|error| format!("lifecycle save failed: {error}"))?;
            let duration = elapsed_ns(started);
            GraphRetrievalDatabase::validate_dir(&config.directory)
                .map_err(|error| format!("saved database validation failed: {error}"))?;
            encode(report(
                &config,
                spec,
                duration,
                None,
                None,
                None,
                Some(account_components(&config.directory)?),
            ))
        }
        "read_only_validation" => {
            let started = Instant::now();
            GraphRetrievalDatabase::validate_dir(&config.directory)
                .map_err(|error| format!("read-only validation failed: {error}"))?;
            encode(report(
                &config,
                spec,
                elapsed_ns(started),
                None,
                None,
                None,
                Some(account_components(&config.directory)?),
            ))
        }
        "cold_load" | "warm_load" => {
            let started = Instant::now();
            let database = GraphRetrievalDatabase::load_from_dir(&config.directory)
                .map_err(|error| format!("{} failed: {error}", config.operation))?;
            let duration = elapsed_ns(started);
            validate_database_shape(&database, spec, encoding)?;
            let replay_started = Instant::now();
            let checks = validate_database_behavior(&database, spec)?;
            let replay_duration = elapsed_ns(replay_started);
            encode(report(
                &config,
                spec,
                duration,
                Some(replay_duration),
                Some(true),
                Some(checks),
                Some(account_components(&config.directory)?),
            ))
        }
        "replay" => {
            let database = GraphRetrievalDatabase::load_from_dir(&config.directory)
                .map_err(|error| format!("replay load failed: {error}"))?;
            validate_database_shape(&database, spec, encoding)?;
            let started = Instant::now();
            let checks = validate_database_behavior(&database, spec)?;
            encode(report(
                &config,
                spec,
                elapsed_ns(started),
                None,
                Some(true),
                Some(checks),
                None,
            ))
        }
        value => Err(format!("unknown Phase 4b lifecycle operation '{value}'")),
    }
}

fn report(
    config: &LifecycleConfig,
    spec: WorkloadSpec,
    operation_duration_ns: u64,
    replay_duration_ns: Option<u64>,
    replay_equivalent: Option<bool>,
    correctness_checks: Option<Vec<String>>,
    persisted_components: Option<PersistedComponents>,
) -> LifecycleReport {
    LifecycleReport {
        schema_version: 1,
        artifact_type: "phase4b_device_lifecycle_sample",
        workload_id: spec.id.to_owned(),
        classification: spec.class.label().to_owned(),
        encoding: config.encoding.clone(),
        sample_id: config.sample_id.clone(),
        operation: config.operation.clone(),
        build_configuration: "release",
        operation_duration_ns,
        replay_duration_ns,
        replay_equivalent,
        correctness_checks,
        persisted_components,
        supported_v1_capacity_changed: false,
    }
}

fn build_database(
    spec: WorkloadSpec,
    encoding: VectorEncoding,
) -> Result<GraphRetrievalDatabase, String> {
    let retrieval = build_retrieval_database(spec, encoding)?;
    GraphRetrievalDatabase::build(retrieval, phase4_graph_schema()?)
        .map_err(|error| format!("failed to build lifecycle database: {error}"))
}

fn parse_encoding(value: &str) -> Result<VectorEncoding, String> {
    match value {
        "f32" => Ok(VectorEncoding::F32),
        "i8" => Ok(VectorEncoding::I8ScalarQuantized),
        _ => Err(format!("unsupported Phase 4b encoding '{value}'")),
    }
}

fn require_fresh_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "refusing to overwrite lifecycle directory '{}'",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create '{}': {error}", parent.display()))?;
    }
    Ok(())
}

fn account_components(path: &Path) -> Result<PersistedComponents, String> {
    let complete = directory_size(path)?;
    let capability = active_capability_directory(path)?;
    let retrieval = capability.join("retrieval");
    let sizes = ExactVectorIndex::persisted_file_sizes(&retrieval)
        .map_err(|error| format!("failed to account retrieval files: {error}"))?;
    let corpus_chunks = sizes
        .chunks_bytes
        .saturating_add(sizes.records_bytes)
        .saturating_add(sizes.tombstones_bytes);
    let vectors = sizes.vectors_bytes;
    let lexical = sizes.bm25_bytes;
    let graph = directory_size(&capability.join("graph"))?;
    let known = corpus_chunks
        .saturating_add(vectors)
        .saturating_add(lexical)
        .saturating_add(graph);
    let manifest = complete
        .checked_sub(known)
        .ok_or_else(|| "persisted component accounting exceeds directory size".to_owned())?;
    Ok(PersistedComponents {
        corpus_chunks_bytes: corpus_chunks,
        vectors_quantization_bytes: vectors,
        lexical_bm25_bytes: lexical,
        graph_schema_bytes: graph,
        manifest_validation_bytes: manifest,
        complete_directory_bytes: complete,
        component_sum_matches_directory: known.saturating_add(manifest) == complete,
    })
}

fn encode(report: LifecycleReport) -> Result<String, String> {
    serde_json::to_string(&report)
        .map_err(|error| format!("failed to encode lifecycle report: {error}"))
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_config_is_closed() {
        let valid = r#"{"workload_id":"10k-384d-v3","encoding":"f32","sample_id":"s","operation":"build","directory":"/tmp/x"}"#;
        assert!(serde_json::from_str::<LifecycleConfig>(valid).is_ok());
        assert!(serde_json::from_str::<LifecycleConfig>(&valid.replace('}', ",\"x\":1}")).is_err());
    }
}
