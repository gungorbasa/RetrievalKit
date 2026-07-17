use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use super::v3_canonical::{canonical_json, sha256, write_canonical_json};
use super::v3_execution::{self, LockedBaselineState};
use super::v3_graph_execution::{self, LockedGraphState};
use super::v3_graph_retrieval_execution::{self, LockedGraphRetrievalState};
use super::v3_runs::{HybridConfiguration, RunContext};
use super::v3_validation::{self, RankingInputValidation, ValidatedCollection};

const ADAPTER_MANIFEST_SHA256: &str =
    "8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6";
const AUTHORIZATION_SCHEMA: &str = "hotpotqa-phase-3b-execution-authorization-v1";
const COLLECTION_ID: &str = "hotpotqa-linked-abstracts-graph-v1-test";
const COLLECTION_ROOT_SHA256: &str =
    "496d21d1c686e2ef3bc36d9820d0cda058f4ca6b82bb029889ed62b48b084f72";
const TEST_POPULATION_SHA256: &str =
    "9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010";
const DERIVED_POPULATION_SHA256: &str =
    "93c252bd743e4084c7c50e9f7dee970af2977967a62c5717ba8edc000101a9d8";
const SELECTED_CONFIGURATION_SHA256: &str =
    "ec4757562140b92f298c85341ab64442dfcb07634da500e8abfe291401b95118";
const SELECTED_PREIMAGE_SHA256: &str =
    "0a96d52338df84033a62a4b5cd14b616023bb07842e8669750fb6c05d4dadf9d";
const BM25_POLICY_SHA256: &str = "988983907ff40ef4638477b37f67de0f26df9f83b4be00314ee99dd6c2db24b1";
const NORMALIZATION_POLICY_SHA256: &str =
    "5393ff7a62243465ae81ce89131c432eb0d0fc982b1e5c786d94f9f48ec1e69e";
const QUANTIZATION_POLICY_SHA256: &str =
    "b7c0bb0252ea789e5810630e2e995aec0a75f635dc4880651db5402c0b2b4881";

#[derive(Debug)]
pub(super) struct LockedArguments {
    pub collection: PathBuf,
    pub configuration_lock: PathBuf,
    pub authorization: PathBuf,
    pub output: PathBuf,
    pub attempt_audit: PathBuf,
}

struct RankingState {
    baseline: LockedBaselineState,
    graph: LockedGraphState,
    graph_retrieval: LockedGraphRetrievalState,
}

pub(super) fn parse_arguments(args: &[String]) -> Result<LockedArguments, String> {
    let mut values = BTreeMap::<&str, PathBuf>::new();
    let mut offset = 0;
    while offset < args.len() {
        let flag = args[offset].as_str();
        if !matches!(
            flag,
            "--collection"
                | "--configuration-lock"
                | "--authorization"
                | "--output"
                | "--attempt-audit"
        ) {
            return Err(format!(
                "locked reporting rejects unsupported or tuning argument '{flag}'"
            ));
        }
        let value = args
            .get(offset + 1)
            .ok_or_else(|| format!("locked reporting argument '{flag}' requires a value"))?;
        if values.insert(flag, PathBuf::from(value)).is_some() {
            return Err(format!("locked reporting argument '{flag}' was repeated"));
        }
        offset += 2;
    }
    let required = |flag: &'static str| {
        values.get(flag).cloned().ok_or_else(|| {
            "usage: vectorkit bench quality-v3-hotpotqa locked-report --collection <test> --configuration-lock <lock.json> --authorization <authorization.json> --output <fresh-root> --attempt-audit <fresh-audit.json>".to_owned()
        })
    };
    Ok(LockedArguments {
        collection: required("--collection")?,
        configuration_lock: required("--configuration-lock")?,
        authorization: required("--authorization")?,
        output: required("--output")?,
        attempt_audit: required("--attempt-audit")?,
    })
}

pub(super) fn execute(arguments: LockedArguments) -> Result<String, String> {
    let repository = repository_root()?;
    require_clean_worktree(&repository)?;
    let collection = arguments.collection.canonicalize().map_err(|error| {
        format!(
            "resolve locked HotpotQA test collection '{}': {error}",
            arguments.collection.display()
        )
    })?;
    if collection.file_name().and_then(|name| name.to_str()) != Some("test") {
        return Err(
            "locked reporting accepts only a collection root named exactly 'test'".to_owned(),
        );
    }
    let output = safe_target_path(&repository, &arguments.output)?;
    let attempt_audit = safe_target_path(&repository, &arguments.attempt_audit)?;
    if output.exists() {
        return Err(format!(
            "locked reporting refuses existing output '{}'",
            output.display()
        ));
    }
    if attempt_audit.exists() {
        return Err(format!(
            "locked reporting refuses a second unauthorized attempt; audit '{}' already exists",
            attempt_audit.display()
        ));
    }

    let lock_bytes = fs::read(&arguments.configuration_lock)
        .map_err(|error| format!("read selected-configuration lock: {error}"))?;
    let lock_hash = sha256(&lock_bytes);
    let hybrid = validate_lock(&lock_bytes, &lock_hash)?;
    let authorization_bytes = fs::read(&arguments.authorization)
        .map_err(|error| format!("read locked execution authorization: {error}"))?;
    let authorization_hash = sha256(&authorization_bytes);
    let authorization = parse_canonical_value("authorization", &authorization_bytes)?;
    let revision = implementation_revision(&repository)?;
    validate_authorization(&authorization, &lock_hash, &revision, &repository)?;
    validate_adapter_and_collection_identity(&collection)?;

    create_attempt_audit(
        &attempt_audit,
        &json!({
            "authorization_sha256":authorization_hash,
            "attempt":1,
            "canonical_result_published":false,
            "implementation_revision":revision,
            "schema_version":1,
            "status":"started"
        }),
    )?;
    let attempt_result = execute_attempt(
        &repository,
        &collection,
        &output,
        &authorization_hash,
        &lock_hash,
        hybrid,
        &revision,
    );
    match attempt_result {
        Ok(summary) => {
            write_canonical_json(
                &attempt_audit,
                &json!({
                    "authorization_sha256":authorization_hash,
                    "attempt":1,
                    "canonical_result_published":true,
                    "implementation_revision":revision,
                    "output_manifest_sha256":summary["output_manifest_sha256"],
                    "ranking_seal_sha256":summary["ranking_seal_sha256"],
                    "schema_version":1,
                    "status":"passed"
                }),
            )?;
            serde_json::to_string_pretty(&summary)
                .map_err(|error| format!("serialize locked reporting summary: {error}"))
        }
        Err(error) => {
            let _ = write_canonical_json(
                &attempt_audit,
                &json!({
                    "authorization_sha256":authorization_hash,
                    "attempt":1,
                    "canonical_result_published":false,
                    "failure":error,
                    "implementation_revision":revision,
                    "schema_version":1,
                    "status":"failed"
                }),
            );
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_attempt(
    repository: &Path,
    collection: &Path,
    output: &Path,
    authorization_hash: &str,
    lock_hash: &str,
    hybrid: HybridConfiguration,
    revision: &Value,
) -> Result<Value, String> {
    let parent = output
        .parent()
        .ok_or_else(|| "locked output has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("create locked output parent: {error}"))?;
    let attempt = parent.join(format!(".phase-3b-attempt-{}", std::process::id()));
    if attempt.exists() {
        return Err(format!(
            "locked attempt staging '{}' already exists",
            attempt.display()
        ));
    }
    fs::create_dir(&attempt).map_err(|error| format!("create locked attempt staging: {error}"))?;
    let result = (|| {
        let primary_root = attempt.join("stage-a-primary");
        let verification_root = attempt.join("stage-a-verification");
        let (primary, primary_seal, opened) = execute_stage_a(
            collection,
            &primary_root,
            authorization_hash,
            lock_hash,
            hybrid,
            revision,
        )?;
        let (_, verification_seal, verification_opened) = execute_stage_a(
            collection,
            &verification_root,
            authorization_hash,
            lock_hash,
            hybrid,
            revision,
        )?;
        compare_directories(&primary_root, &verification_root)?;
        if primary_seal != verification_seal || opened != verification_opened {
            return Err(
                "mandatory Stage A verification did not reproduce the primary seal".to_owned(),
            );
        }

        // This is the first operation in the authorized procedure that may
        // open qrels or evidence. No retrieval function is called below.
        let mut scored_validation = v3_validation::validate(collection)?;
        bind_locked_runs(&mut scored_validation, hybrid, revision)?;
        validate_test_identity(&scored_validation)?;
        let scored_one = attempt.join("stage-b-primary");
        let scored_two = attempt.join("stage-b-verification");
        score_stage_b(
            &scored_validation,
            &primary,
            &primary_root,
            &scored_one,
            authorization_hash,
            &primary_seal,
        )?;
        score_stage_b(
            &scored_validation,
            &primary,
            &primary_root,
            &scored_two,
            authorization_hash,
            &primary_seal,
        )?;
        compare_directories(&scored_one, &scored_two)?;
        let manifest_hash = sha256(
            &fs::read(scored_one.join("manifest.json"))
                .map_err(|error| format!("read finalized locked manifest: {error}"))?,
        );
        fs::rename(&scored_one, output).map_err(|error| {
            format!(
                "atomically publish locked reporting root '{}' from '{}': {error}",
                output.display(),
                scored_one.display()
            )
        })?;
        Ok(json!({
            "authorization_sha256":authorization_hash,
            "attempt_count":1,
            "mandatory_ranking_rerun_equal":true,
            "mandatory_scoring_rerun_equal":true,
            "output":output,
            "output_manifest_sha256":manifest_hash,
            "ranking_seal_sha256":primary_seal,
            "status":"passed"
        }))
    })();
    let _ = fs::remove_dir_all(&attempt);
    let _ = repository;
    result
}

fn execute_stage_a(
    collection: &Path,
    output: &Path,
    authorization_hash: &str,
    lock_hash: &str,
    hybrid: HybridConfiguration,
    revision: &Value,
) -> Result<(RankingState, String, Vec<String>), String> {
    fs::create_dir(output).map_err(|error| format!("create Stage A root: {error}"))?;
    let RankingInputValidation {
        mut validated,
        opened_collection_files,
    } = v3_validation::validate_ranking_inputs(collection)?;
    bind_locked_runs(&mut validated, hybrid, revision)?;
    validate_test_identity(&validated)?;
    let baseline = v3_execution::emit_locked_rankings(&validated, output)?;
    let graph = v3_graph_execution::emit_locked_graph_rankings(&validated, output)?;
    let graph_retrieval =
        v3_graph_retrieval_execution::emit_locked_graph_retrieval_rankings(&validated, output)?;
    write_run_configurations(&validated, output, lock_hash)?;
    write_canonical_json(
        &output.join("stage-a-file-access-audit.json"),
        &json!({
            "forbidden_files_opened":[],
            "opened_collection_files":opened_collection_files,
            "previous_result_inputs":[],
            "schema_version":1,
            "stage":"sealed_ranking",
            "status":"passed"
        }),
    )?;
    write_canonical_json(
        &output.join("execution-environment.json"),
        &execution_environment(revision)?,
    )?;
    write_canonical_json(
        &output.join("stage-a-status.json"),
        &json!({
            "authorization_sha256":authorization_hash,
            "configuration_lock_sha256":lock_hash,
            "labels_opened":false,
            "retrieval_invoked":true,
            "run_count":validated.runs.len(),
            "schema_version":1,
            "status":"valid"
        }),
    )?;
    let inventory = inventory(output, &["ranking-seal.json"])?;
    let seal_preimage = json!({
        "authorization_sha256":authorization_hash,
        "files":inventory,
        "schema_version":1
    });
    let seal = sha256(canonical_json(&seal_preimage)?.as_bytes());
    write_canonical_json(
        &output.join("ranking-seal.json"),
        &json!({
            "file_count":seal_preimage["files"].as_array().unwrap().len(),
            "preimage":seal_preimage,
            "ranking_seal_sha256":seal,
            "schema_version":1,
            "status":"sealed"
        }),
    )?;
    Ok((
        RankingState {
            baseline,
            graph,
            graph_retrieval,
        },
        seal,
        opened_collection_files,
    ))
}

fn score_stage_b(
    validated: &ValidatedCollection,
    state: &RankingState,
    ranking_root: &Path,
    output: &Path,
    authorization_hash: &str,
    ranking_seal: &str,
) -> Result<(), String> {
    copy_directory(ranking_root, output)?;
    verify_ranking_seal(output, ranking_seal)?;
    v3_execution::score_locked_rankings(validated, &state.baseline, output)?;
    v3_graph_execution::score_locked_graph_rankings(validated, &state.graph, output)?;
    v3_graph_retrieval_execution::score_locked_graph_retrieval_rankings(
        validated,
        &state.graph_retrieval,
        output,
    )?;
    run_locked_analysis(validated, output)?;
    write_canonical_json(
        &output.join("stage-b-file-access-audit.json"),
        &json!({
            "opened_label_files":["evidence-judgments.jsonl","expected-paths.jsonl","qrels.tsv"],
            "retrieval_invoked":false,
            "schema_version":1,
            "stage":"scoring_only",
            "status":"passed"
        }),
    )?;
    write_canonical_json(
        &output.join("locked-reporting-summary.json"),
        &json!({
            "authorization_sha256":authorization_hash,
            "declared_counts":{"a":297,"b":297,"c":297,"d":297,"e":297,"f":297,"g":297},
            "executed_counts":{"a":297,"b":297,"c":297,"d":296,"e":296,"f":296,"g":296},
            "labels_opened_after_ranking_seal":true,
            "mandatory_ranking_rerun_equal":true,
            "mandatory_scoring_rerun_equal":true,
            "no_tuning_or_selection":true,
            "path_accuracy":"not_applicable",
            "ranking_seal_sha256":ranking_seal,
            "schema_version":1,
            "status":"valid"
        }),
    )?;
    let files = inventory(output, &["manifest.json"])?;
    let root_hash = sha256(canonical_json(&Value::Array(files.clone()))?.as_bytes());
    write_canonical_json(
        &output.join("manifest.json"),
        &json!({
            "artifact_root_sha256":root_hash,
            "authorization_sha256":authorization_hash,
            "files":files,
            "profile":"locked_deterministic_quality",
            "ranking_seal_sha256":ranking_seal,
            "schema_version":1,
            "status":"valid"
        }),
    )
}

fn run_locked_analysis(validated: &ValidatedCollection, output: &Path) -> Result<(), String> {
    let repository = repository_root()?;
    let script = repository.join("scripts/quality/build_hotpotqa_phase_3_locked_analysis.py");
    let analysis_output = output.join("locked-analysis.json");
    let status = Command::new("python3")
        .args([
            script.as_os_str(),
            std::ffi::OsStr::new("--collection"),
            validated.root.as_os_str(),
            std::ffi::OsStr::new("--artifacts"),
            output.as_os_str(),
            std::ffi::OsStr::new("--output"),
            analysis_output.as_os_str(),
        ])
        .status()
        .map_err(|error| format!("run locked Stage B analysis: {error}"))?;
    if !status.success() {
        return Err("locked Stage B analysis failed".to_owned());
    }
    Ok(())
}

fn bind_locked_runs(
    validated: &mut ValidatedCollection,
    hybrid: HybridConfiguration,
    revision: &Value,
) -> Result<(), String> {
    let context = RunContext {
        graph_schema_sha256: sha256(&validated.bytes[&validated.collection.paths.graph_schema]),
        seed_policy_sha256: sha256(
            &validated.bytes[&validated.collection.paths.seed_policy_manifest],
        ),
        implementation_revision: revision.clone(),
    };
    validated.runs = super::v3_runs::canonical_runs_with_hybrid_configuration(
        &validated.collection,
        &validated.queries,
        &validated.populations,
        &context,
        hybrid,
    )?;
    if validated.runs.len() != 7 {
        return Err(format!(
            "locked reporting expected seven derived-lane runs, actual {}",
            validated.runs.len()
        ));
    }
    Ok(())
}

fn validate_test_identity(validated: &ValidatedCollection) -> Result<(), String> {
    if validated.collection.split != "test"
        || validated.collection.collection_id != COLLECTION_ID
        || validated.collection.collection_version != "1"
        || validated.collection.counts.records != 12_670
        || validated.collection.counts.queries != 297
        || super::v3_population::population_hash(&validated.populations.retrieval)
            != TEST_POPULATION_SHA256
    {
        return Err(format!(
            "locked test collection identity or population differs from contract: split={}({}), collection_id={}({}), version={}({}), records={}({}), queries={}({}), population_sha256={}({})",
            validated.collection.split,
            validated.collection.split != "test",
            validated.collection.collection_id,
            validated.collection.collection_id != COLLECTION_ID,
            validated.collection.collection_version,
            validated.collection.collection_version != "1",
            validated.collection.counts.records,
            validated.collection.counts.records != 12_670,
            validated.collection.counts.queries,
            validated.collection.counts.queries != 297,
            super::v3_population::population_hash(&validated.populations.retrieval),
            super::v3_population::population_hash(&validated.populations.retrieval)
                != TEST_POPULATION_SHA256
        ));
    }
    let derived = validated.populations.successful("hotpotqa-exact-title-v1");
    if super::v3_population::population_hash(&derived) != DERIVED_POPULATION_SHA256
        || derived.len() != 296
    {
        return Err("locked derived execution population differs from contract".to_owned());
    }
    for run in &validated.runs {
        let letter = run.configuration["run_letter"]
            .as_str()
            .ok_or_else(|| "locked run has no letter".to_owned())?;
        let expected = if matches!(letter, "a" | "b" | "c") {
            297
        } else {
            296
        };
        if run.declared.len() != 297 || run.execution.len() != expected {
            return Err(format!("locked Run {letter} population mismatch"));
        }
    }
    Ok(())
}

fn validate_lock(bytes: &[u8], hash: &str) -> Result<HybridConfiguration, String> {
    if hash != SELECTED_CONFIGURATION_SHA256 {
        return Err(format!(
            "selected lock mismatch: expected {SELECTED_CONFIGURATION_SHA256}, actual {hash}"
        ));
    }
    let value = parse_canonical_value("selected lock", bytes)?;
    let selected = &value["selected_candidate"];
    if value["selected_configuration_preimage_sha256"] != SELECTED_PREIMAGE_SHA256
        || value["bm25_policy_sha256"] != BM25_POLICY_SHA256
        || value["normalization_policy_sha256"] != NORMALIZATION_POLICY_SHA256
        || value["quantization_policy_sha256"] != QUANTIZATION_POLICY_SHA256
        || selected["fusion_alpha"] != json!(0.2)
        || selected["fusion_alpha_f32_bits"] != "3e4ccccd"
        || selected["vector_candidate_limit"] != 100
        || selected["keyword_candidate_limit"] != 100
    {
        return Err("selected lock does not contain the immutable C/G configuration".to_owned());
    }
    HybridConfiguration {
        fusion_alpha: 0.2,
        vector_candidate_limit: 100,
        keyword_candidate_limit: 100,
    }
    .validate(12_670)
}

fn validate_authorization(
    authorization: &Value,
    lock_hash: &str,
    revision: &Value,
    repository: &Path,
) -> Result<(), String> {
    if authorization["authorization_schema"] != AUTHORIZATION_SCHEMA
        || authorization["collection"]["collection_id"] != COLLECTION_ID
        || authorization["collection"]["collection_version"] != "1"
        || authorization["collection"]["collection_root_sha256"] != COLLECTION_ROOT_SHA256
        || authorization["collection"]["adapter_manifest_sha256"] != ADAPTER_MANIFEST_SHA256
        || authorization["populations"]["test_sha256"] != TEST_POPULATION_SHA256
        || authorization["populations"]["derived_execution_sha256"] != DERIVED_POPULATION_SHA256
        || authorization["selected_configuration"]["lock_sha256"] != lock_hash
        || authorization["selected_configuration"]["preimage_sha256"] != SELECTED_PREIMAGE_SHA256
        || authorization["selected_configuration"]["fusion_alpha_f32_bits"] != "3e4ccccd"
        || authorization["selected_configuration"]["vector_candidate_limit"] != 100
        || authorization["selected_configuration"]["keyword_candidate_limit"] != 100
        || authorization["matrix"] != json!(["A", "B", "C", "D", "E", "F", "G"])
        || authorization["policies"]["no_retuning"] != true
        || authorization["policies"]["one_logical_reporting_attempt"] != true
    {
        return Err("locked execution authorization differs from the sealed protocol".to_owned());
    }
    let evaluator_commit = authorization["evaluator_implementation_commit"]
        .as_str()
        .ok_or_else(|| "authorization evaluator commit is missing".to_owned())?;
    let ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", evaluator_commit, "HEAD"])
        .current_dir(repository)
        .status()
        .map_err(|error| format!("validate authorized evaluator revision: {error}"))?;
    if !ancestor.success() {
        return Err("authorized evaluator commit is not an ancestor of execution HEAD".to_owned());
    }
    let diff = Command::new("git")
        .args([
            "diff",
            "--quiet",
            evaluator_commit,
            "HEAD",
            "--",
            "crates/vectorkit-cli/src/quality",
            "scripts/quality",
        ])
        .current_dir(repository)
        .status()
        .map_err(|error| format!("compare authorized evaluator sources: {error}"))?;
    if !diff.success() {
        return Err("evaluator or validator changed after authorization binding".to_owned());
    }
    if revision["source_state"] != "clean" {
        return Err("authorized execution source state is not clean".to_owned());
    }
    Ok(())
}

fn validate_adapter_and_collection_identity(collection: &Path) -> Result<(), String> {
    let adapter = collection
        .parent()
        .ok_or_else(|| "test collection has no adapter root".to_owned())?;
    let adapter_bytes = fs::read(adapter.join("adapter-manifest.json"))
        .map_err(|error| format!("read frozen adapter manifest: {error}"))?;
    if sha256(&adapter_bytes) != ADAPTER_MANIFEST_SHA256 {
        return Err("frozen adapter manifest checksum mismatch".to_owned());
    }
    let collection_bytes = fs::read(collection.join("collection.json"))
        .map_err(|error| format!("read locked collection manifest: {error}"))?;
    if sha256(&collection_bytes) != COLLECTION_ROOT_SHA256 {
        return Err("locked collection root identity mismatch".to_owned());
    }
    Ok(())
}

fn write_run_configurations(
    validated: &ValidatedCollection,
    output: &Path,
    lock_hash: &str,
) -> Result<(), String> {
    write_canonical_json(
        &output.join("run-configurations.json"),
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
    )
}

fn verify_ranking_seal(root: &Path, expected: &str) -> Result<(), String> {
    let seal_bytes = fs::read(root.join("ranking-seal.json"))
        .map_err(|error| format!("read ranking seal: {error}"))?;
    let seal = parse_canonical_value("ranking seal", &seal_bytes)?;
    if seal["ranking_seal_sha256"] != expected {
        return Err("ranking seal identity changed before scoring".to_owned());
    }
    let actual = inventory(root, &["ranking-seal.json"])?;
    if seal["preimage"]["files"] != Value::Array(actual) {
        return Err("ranking artifact modified after seal".to_owned());
    }
    let digest = sha256(canonical_json(&seal["preimage"])?.as_bytes());
    if digest != expected {
        return Err("ranking seal preimage checksum mismatch".to_owned());
    }
    Ok(())
}

fn inventory(root: &Path, excluded: &[&str]) -> Result<Vec<Value>, String> {
    fn collect(root: &Path, directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(directory).map_err(|error| {
            format!("read artifact directory '{}': {error}", directory.display())
        })? {
            let entry = entry.map_err(|error| format!("read artifact entry: {error}"))?;
            if entry
                .file_type()
                .map_err(|error| format!("inspect artifact entry: {error}"))?
                .is_dir()
            {
                collect(root, &entry.path(), paths)?;
            } else {
                paths.push(entry.path());
            }
        }
        let _ = root;
        Ok(())
    }
    let mut paths = Vec::new();
    collect(root, root, &mut paths)?;
    let mut files = Vec::new();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .expect("artifact is below root")
            .to_str()
            .ok_or_else(|| "artifact path is not UTF-8".to_owned())?
            .to_owned();
        if excluded.contains(&relative.as_str()) {
            continue;
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("read artifact '{}': {error}", path.display()))?;
        files.push(json!({"bytes":bytes.len(),"path":relative,"sha256":sha256(&bytes)}));
    }
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(files)
}

fn compare_directories(left: &Path, right: &Path) -> Result<(), String> {
    let left_inventory = inventory(left, &[])?;
    let right_inventory = inventory(right, &[])?;
    if left_inventory != right_inventory {
        return Err(format!(
            "locked deterministic roots '{}' and '{}' are not byte-identical",
            left.display(),
            right.display()
        ));
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err(format!(
            "refusing to overwrite '{}': existing output",
            destination.display()
        ));
    }
    fs::create_dir(destination)
        .map_err(|error| format!("create scored root '{}': {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("read ranking root '{}': {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("read ranking entry: {error}"))?;
        let target = destination.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| format!("inspect ranking entry: {error}"))?
            .is_dir()
        {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)
                .map_err(|error| format!("copy sealed ranking artifact: {error}"))?;
        }
    }
    Ok(())
}

fn parse_canonical_value(label: &str, bytes: &[u8]) -> Result<Value, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("parse {label}: {error}"))?;
    if bytes != format!("{}\n", canonical_json(&value)?).as_bytes() {
        return Err(format!("{label} is not canonical JSON plus LF"));
    }
    Ok(value)
}

fn execution_environment(revision: &Value) -> Result<Value, String> {
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .map_err(|error| format!("identify rustc: {error}"))?;
    Ok(json!({
        "architecture":std::env::consts::ARCH,
        "determinism_environment":{
            "LANG":std::env::var("LANG").ok(),
            "LC_ALL":std::env::var("LC_ALL").ok(),
            "RAYON_NUM_THREADS":std::env::var("RAYON_NUM_THREADS").ok(),
            "TZ":std::env::var("TZ").ok()
        },
        "implementation_revision":revision,
        "operating_system":std::env::consts::OS,
        "rustc":String::from_utf8_lossy(&rustc.stdout).trim(),
        "schema_version":1
    }))
}

fn implementation_revision(repository: &Path) -> Result<Value, String> {
    let git_commit = command_text(repository, &["rev-parse", "HEAD"])?;
    let source_tree_sha256 = command_text(repository, &["rev-parse", "HEAD^{tree}"])?;
    let executable =
        std::env::current_exe().map_err(|error| format!("resolve locked executable: {error}"))?;
    let binary = fs::read(&executable)
        .map_err(|error| format!("read locked executable '{}': {error}", executable.display()))?;
    Ok(json!({
        "binary_sha256":sha256(&binary),
        "executable":executable,
        "git_commit":git_commit,
        "source_state":"clean",
        "source_tree_sha256":source_tree_sha256
    }))
}

fn require_clean_worktree(repository: &Path) -> Result<(), String> {
    let status = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("inspect locked reporting worktree: {error}"))?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("locked reporting requires a clean committed worktree".to_owned());
    }
    Ok(())
}

fn command_text(repository: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .output()
        .map_err(|error| format!("execute git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!("git {} failed", args.join(" ")));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|_| format!("git {} output is not UTF-8", args.join(" ")))
}

fn repository_root() -> Result<PathBuf, String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| format!("resolve repository root: {error}"))
}

fn safe_target_path(repository: &Path, requested: &Path) -> Result<PathBuf, String> {
    let allowed = repository.join("target/benchmarks/hotpotqa-phase-3b");
    fs::create_dir_all(&allowed)
        .map_err(|error| format!("create Phase 3b target root: {error}"))?;
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
            "locked output must be beneath '{}', actual '{}'",
            allowed.display(),
            requested.display()
        ));
    }
    Ok(requested)
}

fn create_attempt_audit(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create attempt audit parent: {error}"))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "create one-shot attempt audit '{}': {error}",
                path.display()
            )
        })?;
    file.write_all(format!("{}\n", canonical_json(value)?).as_bytes())
        .map_err(|error| format!("write one-shot attempt audit: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_tuning_and_override_arguments() {
        for flag in [
            "--search-space",
            "--fusion-alpha",
            "--vector-candidate-limit",
            "--keyword-candidate-limit",
            "--development-results",
            "--exclude-query",
        ] {
            let error = parse_arguments(&[flag.to_owned(), "value".to_owned()]).unwrap_err();
            assert!(error.contains("rejects unsupported or tuning"));
        }
    }

    #[test]
    fn ranking_labels_are_a_closed_forbidden_set() {
        assert_eq!(
            super::super::v3_validation::LABEL_PATHS,
            [
                "evidence-judgments.jsonl",
                "expected-paths.jsonl",
                "qrels.tsv"
            ]
        );
    }

    #[test]
    fn frozen_test_ranking_inputs_have_locked_populations_without_labels() {
        let repository = repository_root().unwrap();
        let root = repository
            .join("target/benchmarks/public-collections/hotpotqa-linked-abstracts-graph-v1/test");
        if !root.exists() {
            return;
        }
        let mut ranking = v3_validation::validate_ranking_inputs(&root)
            .unwrap()
            .validated;
        let revision = json!({
            "binary_sha256":"test",
            "executable":"test",
            "git_commit":"test",
            "source_state":"clean",
            "source_tree_sha256":"test"
        });
        bind_locked_runs(
            &mut ranking,
            HybridConfiguration {
                fusion_alpha: 0.2,
                vector_candidate_limit: 100,
                keyword_candidate_limit: 100,
            },
            &revision,
        )
        .unwrap();
        assert_eq!(
            super::super::v3_population::population_hash(&ranking.populations.retrieval).as_bytes(),
            TEST_POPULATION_SHA256.as_bytes()
        );
        validate_test_identity(&ranking).unwrap();
        assert!(ranking.qrels.is_empty());
        assert!(ranking.evidence.is_empty());
        assert!(ranking.expected_paths.is_empty());
    }
}
