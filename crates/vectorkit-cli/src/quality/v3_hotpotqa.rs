use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use serde_json::{json, Value};

use super::v3_canonical::{canonical_json, sha256};
use super::v3_runs::{HybridConfiguration, RunContext};

const ADAPTER_MANIFEST_SHA256: &str =
    "8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6";
const DEVELOPMENT_COLLECTION_ID: &str = "hotpotqa-linked-abstracts-graph-v1-development";
const DEVELOPMENT_POPULATION_SHA256: &str =
    "1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f";
const SEARCH_SPACE_SHA256: &str =
    "30a93141c0b36d446617342ae846ff4174ff1f8b0f0f9cf008882ed6f3cbdeca";
const SELECTED_CONFIGURATION_SHA256: &str =
    "ec4757562140b92f298c85341ab64442dfcb07634da500e8abfe291401b95118";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchSpace {
    candidates: Vec<SearchCandidate>,
    collection: Value,
    grid: Value,
    invariants: Value,
    protocol_schema: String,
    selection_objective: Value,
    selection_source: Value,
    shared_lock: Value,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchCandidate {
    fusion_alpha: f64,
    keyword_candidate_limit: usize,
    vector_candidate_limit: usize,
}

pub(crate) fn run_cli(args: &[String]) -> Result<String, String> {
    let [mode, rest @ ..] = args else {
        return Err(usage());
    };
    match mode.as_str() {
        "tune" => run_tuning(rest),
        "matrix" => run_matrix(rest),
        value => Err(format!(
            "unknown quality-v3-hotpotqa mode '{value}'; {}",
            usage()
        )),
    }
}

fn run_matrix(args: &[String]) -> Result<String, String> {
    let mut collection = None;
    let mut lock = None;
    let mut output = None;
    let mut offset = 0;
    while offset < args.len() {
        match args[offset].as_str() {
            "--collection" => {
                collection = Some(PathBuf::from(required_value(args, offset)?));
                offset += 2;
            }
            "--configuration-lock" => {
                lock = Some(PathBuf::from(required_value(args, offset)?));
                offset += 2;
            }
            "--output" => {
                output = Some(PathBuf::from(required_value(args, offset)?));
                offset += 2;
            }
            value => return Err(format!("unknown HotpotQA matrix argument '{value}'")),
        }
    }
    let collection = collection.ok_or_else(matrix_usage)?;
    let lock = lock.ok_or_else(matrix_usage)?;
    let output = output.ok_or_else(matrix_usage)?;
    reject_sealed_test_path(&collection)?;
    reject_sealed_test_path(&lock)?;
    let collection = collection.canonicalize().map_err(|error| {
        format!(
            "failed to resolve HotpotQA development collection '{}': {error}",
            collection.display()
        )
    })?;
    reject_sealed_test_path(&collection)?;
    if collection.file_name().and_then(|name| name.to_str()) != Some("development") {
        return Err("HotpotQA Phase 3a matrix requires the development root".to_owned());
    }
    let lock_bytes = fs::read(&lock)
        .map_err(|error| format!("read HotpotQA selected-configuration lock: {error}"))?;
    let lock_hash = sha256(&lock_bytes);
    if lock_hash != SELECTED_CONFIGURATION_SHA256 {
        return Err(format!(
            "HotpotQA selected-configuration lock changed: expected {SELECTED_CONFIGURATION_SHA256}, actual {lock_hash}"
        ));
    }
    let lock_value: Value = serde_json::from_slice(&lock_bytes)
        .map_err(|error| format!("parse HotpotQA selected-configuration lock: {error}"))?;
    if lock_bytes != format!("{}\n", canonical_json(&lock_value)?).as_bytes()
        || lock_value["protocol_schema"] != "hotpotqa-phase-3-selected-configuration-v1"
        || lock_value["search_space_sha256"] != SEARCH_SPACE_SHA256
        || lock_value["selection_source"] != "development Run C alone"
        || lock_value["test_results_status"] != "unavailable and not inspected"
    {
        return Err(
            "HotpotQA selected-configuration lock is not the frozen canonical lock".to_owned(),
        );
    }
    let selected = &lock_value["selected_candidate"];
    if selected["fusion_alpha_f32_bits"] != "3e4ccccd" {
        return Err("HotpotQA selected alpha F32 bits changed".to_owned());
    }
    let hybrid = HybridConfiguration {
        fusion_alpha: selected["fusion_alpha"]
            .as_f64()
            .ok_or_else(|| "HotpotQA selected alpha is missing".to_owned())?,
        vector_candidate_limit: selected["vector_candidate_limit"]
            .as_u64()
            .ok_or_else(|| "HotpotQA selected vector limit is missing".to_owned())?
            as usize,
        keyword_candidate_limit: selected["keyword_candidate_limit"]
            .as_u64()
            .ok_or_else(|| "HotpotQA selected keyword limit is missing".to_owned())?
            as usize,
    }
    .validate(12_670)?;

    let repository = repository_root()?;
    let output = safe_output_path(&repository, &output)?;
    if output.exists() {
        return Err(format!(
            "HotpotQA Phase 3a matrix output '{}' already exists",
            output.display()
        ));
    }
    let mut validated = super::v3_validation::validate(&collection)?;
    validate_development_identity(&validated)?;
    let context = RunContext {
        graph_schema_sha256: sha256(&validated.bytes[&validated.collection.paths.graph_schema]),
        seed_policy_sha256: sha256(
            &validated.bytes[&validated.collection.paths.seed_policy_manifest],
        ),
        implementation_revision: implementation_revision(&repository)?,
    };
    validated.runs = super::v3_runs::canonical_runs_with_hybrid_configuration(
        &validated.collection,
        &validated.queries,
        &validated.populations,
        &context,
        hybrid,
    )?;
    validate_development_matrix(&validated, hybrid)?;

    let staging_parent = output.parent().ok_or_else(|| {
        format!(
            "HotpotQA Phase 3a output '{}' has no parent",
            output.display()
        )
    })?;
    fs::create_dir_all(staging_parent)
        .map_err(|error| format!("create HotpotQA matrix parent: {error}"))?;
    let staging = staging_parent.join(format!(
        ".hotpotqa-phase-3a-matrix-staging-{}",
        std::process::id()
    ));
    if staging.exists() {
        return Err(format!(
            "HotpotQA matrix staging path '{}' already exists",
            staging.display()
        ));
    }
    fs::create_dir(&staging).map_err(|error| format!("create HotpotQA matrix staging: {error}"))?;
    let staged_output = staging.join("artifacts");
    let result = (|| {
        let status = super::v3_execution::emit_qualification(&validated, &staged_output)?;
        if status.qualification != "valid" {
            return Err("HotpotQA development A-G matrix contains invalid execution".to_owned());
        }
        for relative in [
            "evidence-judgments.jsonl",
            "expected-paths.jsonl",
            "exclusions.jsonl",
        ] {
            fs::write(staged_output.join(relative), &validated.bytes[relative]).map_err(
                |error| format!("copy HotpotQA judgment artifact '{relative}': {error}"),
            )?;
        }
        super::v3_canonical::write_canonical_json(
            &staged_output.join("run-configurations.json"),
            &json!({
                "configuration_lock_sha256":lock_hash,
                "runs":validated.runs.iter().map(|run|json!({
                    "configuration":run.configuration,
                    "declared_population_sha256":run.declared_hash(),
                    "execution_population_sha256":run.execution_hash(),
                    "logical_run_sha256":run.logical_run_sha256,
                    "run_id":run.run_id
                })).collect::<Vec<_>>(),
                "schema_version":1
            }),
        )?;
        super::v3_canonical::write_canonical_json(
            &staged_output.join("test-access-audit.json"),
            &json!({
                "collection_id":validated.collection.collection_id,
                "opened_splits":["development"],
                "schema_version":1,
                "test_artifacts_generated":false,
                "test_collection_opened":false,
                "test_metrics_inspected":false
            }),
        )?;
        super::v3_canonical::write_canonical_json(
            &staged_output.join("phase-3a-development-matrix.json"),
            &json!({
                "configuration_lock_sha256":lock_hash,
                "declared_counts":{"a":603,"b":603,"c":603,"d":603,"e":603,"f":603,"g":603},
                "executed_counts":{"a":603,"b":603,"c":603,"d":599,"e":599,"f":599,"g":599},
                "excluded_pre_freeze_counts":{"a":0,"b":0,"c":0,"d":4,"e":4,"f":4,"g":4},
                "phase":"3a-development",
                "run_ids":validated.runs.iter().map(|run|json!({"letter":run.configuration["run_letter"],"run_id":run.run_id})).collect::<Vec<_>>(),
                "schema_version":1,
                "status":"valid",
                "test_split_executed":false
            }),
        )?;
        let files = matrix_file_inventory(&staged_output)?;
        super::v3_canonical::write_canonical_json(
            &staged_output.join("manifest.json"),
            &json!({
                "collection_id":validated.collection.collection_id,
                "configuration_lock_sha256":lock_hash,
                "files":files,
                "profile":"deterministic_quality",
                "run_count":validated.runs.len(),
                "schema_version":1,
                "status":"valid"
            }),
        )?;
        fs::rename(&staged_output, &output).map_err(|error| {
            format!(
                "publish HotpotQA matrix '{}' from '{}': {error}",
                output.display(),
                staged_output.display()
            )
        })?;
        Ok(status)
    })();
    let _ = fs::remove_dir(&staging);
    let status = result?;
    serde_json::to_string_pretty(&json!({
        "configuration_lock_sha256":lock_hash,
        "output":output,
        "phase_1_2a":status.phase_1_2a,
        "phase_1_2b":status.phase_1_2b,
        "phase_1_2c":status.phase_1_2c,
        "run_count":validated.runs.len(),
        "run_ids":validated.runs.iter().map(|run|run.run_id.clone()).collect::<Vec<_>>(),
        "status":status.qualification,
        "test_split_accessed":false
    }))
    .map_err(|error| format!("serialize HotpotQA matrix result: {error}"))
}

fn run_tuning(args: &[String]) -> Result<String, String> {
    let mut collection = None;
    let mut search_space = None;
    let mut output = None;
    let mut offset = 0;
    while offset < args.len() {
        match args[offset].as_str() {
            "--collection" => {
                collection = Some(PathBuf::from(required_value(args, offset)?));
                offset += 2;
            }
            "--search-space" => {
                search_space = Some(PathBuf::from(required_value(args, offset)?));
                offset += 2;
            }
            "--output" => {
                output = Some(PathBuf::from(required_value(args, offset)?));
                offset += 2;
            }
            value => return Err(format!("unknown HotpotQA tuning argument '{value}'")),
        }
    }
    let collection = collection.ok_or_else(usage)?;
    let search_space = search_space.ok_or_else(usage)?;
    let output = output.ok_or_else(usage)?;

    reject_sealed_test_path(&collection)?;
    let collection = collection.canonicalize().map_err(|error| {
        format!(
            "failed to resolve HotpotQA development collection '{}': {error}",
            collection.display()
        )
    })?;
    reject_sealed_test_path(&collection)?;
    if collection.file_name().and_then(|name| name.to_str()) != Some("development") {
        return Err(format!(
            "HotpotQA tuning requires a root whose final component is exactly 'development', actual '{}'",
            collection.display()
        ));
    }
    let adapter_root = collection.parent().ok_or_else(|| {
        format!(
            "HotpotQA development root '{}' has no adapter parent",
            collection.display()
        )
    })?;
    let adapter_manifest = fs::read(adapter_root.join("adapter-manifest.json"))
        .map_err(|error| format!("read frozen HotpotQA adapter manifest: {error}"))?;
    let adapter_hash = sha256(&adapter_manifest);
    if adapter_hash != ADAPTER_MANIFEST_SHA256 {
        return Err(format!(
            "HotpotQA adapter manifest checksum mismatch: expected {ADAPTER_MANIFEST_SHA256}, actual {adapter_hash}"
        ));
    }

    let (search, search_hash) = read_search_space(&search_space)?;
    let repository = repository_root()?;
    let output = safe_output_path(&repository, &output)?;
    let mut validated = super::v3_validation::validate(&collection)?;
    if validated.collection.split != "development"
        || validated.collection.collection_id != DEVELOPMENT_COLLECTION_ID
        || validated.collection.collection_version != "1"
        || validated.collection.counts.records != 12_670
        || validated.collection.counts.chunks != 12_670
        || validated.collection.counts.queries != 603
        || validated.collection.counts.qrel_rows != 1_206
        || validated.collection.counts.evidence_rows != 603
    {
        return Err("HotpotQA tuning collection identity/counts differ from the frozen development contract".to_owned());
    }
    let population = super::v3_population::population_hash(&validated.populations.retrieval);
    if population != DEVELOPMENT_POPULATION_SHA256 {
        return Err(format!(
            "HotpotQA development population checksum mismatch: expected {DEVELOPMENT_POPULATION_SHA256}, actual {population}"
        ));
    }
    validate_search_space_context(&search, &search_hash)?;
    let candidates = search
        .candidates
        .iter()
        .map(|candidate| HybridConfiguration {
            fusion_alpha: candidate.fusion_alpha,
            vector_candidate_limit: candidate.vector_candidate_limit,
            keyword_candidate_limit: candidate.keyword_candidate_limit,
        })
        .collect::<Vec<_>>();
    let context = RunContext {
        graph_schema_sha256: sha256(&validated.bytes[&validated.collection.paths.graph_schema]),
        seed_policy_sha256: sha256(
            &validated.bytes[&validated.collection.paths.seed_policy_manifest],
        ),
        implementation_revision: implementation_revision(&repository)?,
    };
    validated.runs = super::v3_runs::canonical_runs_with_hybrid_configuration(
        &validated.collection,
        &validated.queries,
        &validated.populations,
        &context,
        HybridConfiguration {
            fusion_alpha: 0.6,
            vector_candidate_limit: 50,
            keyword_candidate_limit: 50,
        },
    )?;
    let selected = super::v3_execution::emit_hotpotqa_tuning_search(
        &validated,
        &candidates,
        &context,
        &search_hash,
        &output,
    )?;
    serde_json::to_string_pretty(&json!({
        "adapter_manifest_sha256":adapter_hash,
        "candidate_count":candidates.len(),
        "collection_id":validated.collection.collection_id,
        "development_population_sha256":population,
        "output":output,
        "search_space_sha256":search_hash,
        "selected":selected["selected"],
        "status":"valid",
        "test_split_accessed":false
    }))
    .map_err(|error| format!("serialize HotpotQA tuning result: {error}"))
}

fn required_value(args: &[String], offset: usize) -> Result<&str, String> {
    args.get(offset + 1).map(String::as_str).ok_or_else(usage)
}

fn usage() -> String {
    "usage: vectorkit bench quality-v3-hotpotqa tune --collection <development> --search-space <phase-3-development-search-space.json> --output <target/benchmarks/hotpotqa-phase-3a/tuning>".to_owned()
}

fn matrix_usage() -> String {
    "usage: vectorkit bench quality-v3-hotpotqa matrix --collection <development> --configuration-lock <phase-3-selected-configuration.json> --output <target/benchmarks/hotpotqa-phase-3a/development-matrix>".to_owned()
}

fn validate_development_identity(
    validated: &super::v3_validation::ValidatedCollection,
) -> Result<(), String> {
    if validated.collection.split != "development"
        || validated.collection.collection_id != DEVELOPMENT_COLLECTION_ID
        || validated.collection.collection_version != "1"
        || validated.collection.counts.records != 12_670
        || validated.collection.counts.chunks != 12_670
        || validated.collection.counts.queries != 603
        || validated.collection.counts.qrel_rows != 1_206
        || validated.collection.counts.evidence_rows != 603
        || super::v3_population::population_hash(&validated.populations.retrieval)
            != DEVELOPMENT_POPULATION_SHA256
    {
        return Err("HotpotQA development collection differs from the frozen contract".to_owned());
    }
    Ok(())
}

fn validate_development_matrix(
    validated: &super::v3_validation::ValidatedCollection,
    hybrid: HybridConfiguration,
) -> Result<(), String> {
    if validated.runs.len() != 7 {
        return Err(format!(
            "HotpotQA development matrix expected seven runs, actual {}",
            validated.runs.len()
        ));
    }
    for run in &validated.runs {
        let letter = run.configuration["run_letter"]
            .as_str()
            .ok_or_else(|| "HotpotQA matrix run lacks letter".to_owned())?;
        let expected_execution = if matches!(letter, "a" | "b" | "c") {
            603
        } else {
            599
        };
        if run.declared.len() != 603 || run.execution.len() != expected_execution {
            return Err(format!(
                "HotpotQA Run {letter} population mismatch: declared {}, execution {}",
                run.declared.len(),
                run.execution.len()
            ));
        }
        if matches!(letter, "c" | "g")
            && (run.configuration["fusion_alpha"] != json!(hybrid.fusion_alpha)
                || run.configuration["candidate_limits"]["vector"] != hybrid.vector_candidate_limit
                || run.configuration["candidate_limits"]["keyword"]
                    != hybrid.keyword_candidate_limit)
        {
            return Err(format!(
                "HotpotQA Run {letter} differs from the configuration lock"
            ));
        }
    }
    let c = validated
        .runs
        .iter()
        .find(|run| run.configuration["run_letter"] == "c")
        .unwrap();
    let g = validated
        .runs
        .iter()
        .find(|run| run.configuration["run_letter"] == "g")
        .unwrap();
    for field in ["bm25_policy", "candidate_limits", "fusion_alpha"] {
        if c.configuration[field] != g.configuration[field] {
            return Err(format!("HotpotQA C/G field '{field}' mismatch"));
        }
    }
    Ok(())
}

fn matrix_file_inventory(root: &Path) -> Result<Vec<Value>, String> {
    let mut files = Vec::new();
    for path in matrix_files(root)? {
        let bytes = fs::read(&path)
            .map_err(|error| format!("read matrix artifact '{}': {error}", path.display()))?;
        let relative = path
            .strip_prefix(root)
            .expect("matrix artifact is beneath root")
            .to_str()
            .ok_or_else(|| "matrix artifact path is not UTF-8".to_owned())?
            .to_owned();
        files.push(json!({"bytes":bytes.len(),"path":relative,"sha256":sha256(&bytes)}));
    }
    files.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap()
            .cmp(right["path"].as_str().unwrap())
    });
    Ok(files)
}

fn matrix_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn collect(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("read matrix directory '{}': {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("read matrix entry: {error}"))?;
            if entry
                .file_type()
                .map_err(|error| format!("inspect matrix entry: {error}"))?
                .is_dir()
            {
                collect(&entry.path(), output)?;
            } else if entry.file_name() != "manifest.json" {
                output.push(entry.path());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    collect(root, &mut files)?;
    Ok(files)
}

fn reject_sealed_test_path(path: &Path) -> Result<(), String> {
    if path
        .components()
        .any(|component| matches!(component, Component::Normal(value) if value == "test"))
    {
        return Err(format!(
            "HotpotQA tuning refuses every path containing a 'test' component before opening collection files: '{}'",
            path.display()
        ));
    }
    Ok(())
}

fn read_search_space(path: &Path) -> Result<(SearchSpace, String), String> {
    reject_sealed_test_path(path)?;
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "read HotpotQA development search space '{}': {error}",
            path.display()
        )
    })?;
    let hash = sha256(&bytes);
    if hash != SEARCH_SPACE_SHA256 {
        return Err(format!(
            "HotpotQA development search space changed after pre-registration: expected {SEARCH_SPACE_SHA256}, actual {hash}"
        ));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parse HotpotQA development search space: {error}"))?;
    let canonical = format!("{}\n", canonical_json(&value)?);
    if bytes != canonical.as_bytes() {
        return Err("HotpotQA development search space is not canonical JSON plus LF".to_owned());
    }
    let search: SearchSpace = serde_json::from_value(value)
        .map_err(|error| format!("decode HotpotQA development search space: {error}"))?;
    Ok((search, hash))
}

fn validate_search_space_context(search: &SearchSpace, hash: &str) -> Result<(), String> {
    if hash != SEARCH_SPACE_SHA256
        || search.protocol_schema != "hotpotqa-phase-3-development-search-space-v1"
        || search.candidates.len() != 36
        || search.collection["split"] != "development"
        || search.collection["collection_id"] != DEVELOPMENT_COLLECTION_ID
        || search.collection["development_population_sha256"] != DEVELOPMENT_POPULATION_SHA256
        || search.selection_source["run_letter"] != "c"
        || search.selection_source["scope"] != "whole"
        || search.shared_lock["run_c_and_g_identical"] != true
    {
        return Err(
            "HotpotQA development search-space context differs from the pre-registered contract"
                .to_owned(),
        );
    }
    let _ = (
        &search.grid,
        &search.invariants,
        &search.selection_objective,
    );
    let mut canonical = search
        .candidates
        .iter()
        .map(|candidate| {
            canonical_json(&json!({
                "fusion_alpha":candidate.fusion_alpha,
                "keyword_candidate_limit":candidate.keyword_candidate_limit,
                "vector_candidate_limit":candidate.vector_candidate_limit
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    canonical.sort();
    canonical.dedup();
    if canonical.len() != search.candidates.len() {
        return Err("HotpotQA development search space contains duplicate candidates".to_owned());
    }
    for candidate in &search.candidates {
        HybridConfiguration {
            fusion_alpha: candidate.fusion_alpha,
            vector_candidate_limit: candidate.vector_candidate_limit,
            keyword_candidate_limit: candidate.keyword_candidate_limit,
        }
        .validate(12_670)?;
    }
    Ok(())
}

fn repository_root() -> Result<PathBuf, String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| format!("resolve repository root: {error}"))
}

fn safe_output_path(repository: &Path, requested: &Path) -> Result<PathBuf, String> {
    reject_sealed_test_path(requested)?;
    let allowed = repository.join("target/benchmarks/hotpotqa-phase-3a");
    fs::create_dir_all(&allowed)
        .map_err(|error| format!("create HotpotQA Phase 3a target root: {error}"))?;
    let allowed = allowed
        .canonicalize()
        .map_err(|error| format!("resolve HotpotQA Phase 3a target root: {error}"))?;
    let requested = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        repository.join(requested)
    };
    if requested
        .components()
        .any(|component| matches!(component, Component::ParentDir))
        || !requested.starts_with(&allowed)
    {
        return Err(format!(
            "HotpotQA Phase 3a output must be a fresh path beneath '{}', actual '{}'",
            allowed.display(),
            requested.display()
        ));
    }
    Ok(requested)
}

fn implementation_revision(repository: &Path) -> Result<Value, String> {
    let status = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("inspect HotpotQA tuning worktree: {error}"))?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("HotpotQA tuning requires a clean committed worktree".to_owned());
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("resolve HotpotQA tuning revision: {error}"))?;
    if !revision.status.success() {
        return Err("failed to resolve HotpotQA tuning Git revision".to_owned());
    }
    let git_commit = String::from_utf8(revision.stdout)
        .map_err(|_| "HotpotQA tuning Git revision is not UTF-8".to_owned())?
        .trim()
        .to_owned();
    let executable = std::env::current_exe()
        .map_err(|error| format!("resolve HotpotQA tuning executable: {error}"))?;
    let binary = fs::read(&executable).map_err(|error| {
        format!(
            "read HotpotQA tuning executable '{}': {error}",
            executable.display()
        )
    })?;
    Ok(json!({
        "binary_sha256":sha256(&binary),
        "git_commit":git_commit,
        "source_sha256":Value::Null
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_rejects_test_component_before_collection_access() {
        let error = reject_sealed_test_path(Path::new("/sealed/test")).unwrap_err();
        assert!(error.contains("before opening collection files"));
    }

    #[test]
    fn tuning_accepts_development_component() {
        reject_sealed_test_path(Path::new("/sealed/development")).unwrap();
    }
}
