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
        value => Err(format!(
            "unknown quality-v3-hotpotqa mode '{value}'; {}",
            usage()
        )),
    }
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
