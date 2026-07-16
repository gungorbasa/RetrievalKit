use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::v3_canonical::{
    canonical_json, canonical_json_line, first_difference, sha256, write_canonical_json,
};
use super::v3_population::population_hash;
use super::v3_runs::{bm25_policy, normalization_policy, quantization_policy, RunIdentity};
use super::v3_validation::{validate, ValidatedCollection};

static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn run_cli(args: &[String]) -> Result<String, String> {
    let mut collection = None;
    let mut artifacts = None;
    let mut verify_rerun = false;
    let mut offset = 0;
    while offset < args.len() {
        match args[offset].as_str() {
            "--collection" => {
                collection =
                    Some(PathBuf::from(args.get(offset + 1).ok_or_else(|| {
                        "missing value for '--collection'".to_owned()
                    })?));
                offset += 2;
            }
            "--foundation-artifacts" => {
                artifacts = Some(PathBuf::from(args.get(offset + 1).ok_or_else(|| {
                    "missing value for '--foundation-artifacts'".to_owned()
                })?));
                offset += 2;
            }
            "--verify-rerun" => {
                verify_rerun = true;
                offset += 1;
            }
            value => return Err(format!("unknown quality-v3 argument '{value}'")),
        }
    }
    let collection = collection.ok_or_else(|| {
        "usage: vectorkit bench quality-v3 --collection <v3-directory> [--foundation-artifacts <directory>] [--verify-rerun]".to_owned()
    })?;
    let validated = validate(&collection)?;
    if let Some(path) = artifacts {
        emit_foundation(&validated, &path)?;
    }
    if verify_rerun {
        verify_deterministic_rerun(&collection)?;
    }
    let result = json!({
        "collection_id":validated.collection.collection_id,
        "collection_version":validated.collection.collection_version,
        "foundation_only":true,
        "normative_fixture_bytes":2135,
        "normative_fixture_sha256":"4d7b920b8ae591f0c05cd41abbc36c50210bbf23e6bfa0e09b4eebbffdea4f46",
        "query_population_sha256":population_hash(&validated.populations.q),
        "rerun_verified":verify_rerun,
        "run_count":validated.runs.len(),
        "status":"valid"
    });
    serde_json::to_string_pretty(&result)
        .map_err(|error| format!("failed to serialize V3 validation result: {error}"))
}

pub(super) fn emit_foundation(
    validated: &ValidatedCollection,
    output: &Path,
) -> Result<(), String> {
    if output.exists() {
        return Err(format!(
            "foundation artifact root '{}' already exists; a fresh directory is required",
            output.display()
        ));
    }
    fs::create_dir_all(output).map_err(|error| {
        format!(
            "failed to create foundation artifact root '{}': {error}",
            output.display()
        )
    })?;
    let collection_output = output.join("validated-collection");
    fs::create_dir_all(collection_output.join("manifests")).map_err(|error| {
        format!(
            "failed to create validated collection directory '{}': {error}",
            collection_output.display()
        )
    })?;
    fs::write(
        collection_output.join("collection.json"),
        fs::read(validated.root.join("collection.json"))
            .map_err(|error| format!("failed to reread collection.json: {error}"))?,
    )
    .map_err(|error| format!("failed to write validated collection.json: {error}"))?;
    for (path, bytes) in &validated.bytes {
        fs::write(collection_output.join(path), bytes).map_err(|error| {
            format!(
                "failed to write validated collection copy '{}': {error}",
                path
            )
        })?;
    }

    write_canonical_json(
        &output.join("populations.json"),
        &population_artifact(validated),
    )?;
    write_runs(&output.join("run-configurations.jsonl"), &validated.runs)?;
    write_canonical_json(
        &output.join("generation-fingerprints.json"),
        &generation_fingerprints(validated)?,
    )?;
    write_canonical_json(
        &output.join("determinism-context.json"),
        &determinism_context(),
    )?;
    write_foundation_manifest(output, validated)?;
    Ok(())
}

pub(super) fn verify_deterministic_rerun(collection: &Path) -> Result<(), String> {
    let first = TemporaryDirectory::new("vectorkit-v3-foundation-a")?;
    let second = TemporaryDirectory::new("vectorkit-v3-foundation-b")?;
    let first_output = first.path.join("artifacts");
    let second_output = second.path.join("artifacts");
    emit_foundation(&validate(collection)?, &first_output)?;
    emit_foundation(&validate(collection)?, &second_output)?;
    compare_directories(&first_output, &second_output)
}

fn population_artifact(validated: &ValidatedCollection) -> Value {
    let mut populations = vec![
        population_row("Q", &validated.populations.q),
        population_row("R", &validated.populations.retrieval),
        population_row("S_exp", &validated.populations.explicit),
        population_row("X_exp", &validated.populations.explicit),
    ];
    for (policy, declared) in &validated.populations.derived_declared {
        populations.push(population_row(
            &format!("F_{policy}"),
            &validated.populations.derived_failed[policy],
        ));
        populations.push(population_row(
            &format!("S_{policy}"),
            &validated.populations.successful(policy),
        ));
        populations.push(population_row(&format!("X_{policy}"), declared));
    }
    populations.sort_by(|left, right| {
        left["name"]
            .as_str()
            .unwrap()
            .as_bytes()
            .cmp(right["name"].as_str().unwrap().as_bytes())
    });
    json!({
        "collection_id":validated.collection.collection_id,
        "collection_version":validated.collection.collection_version,
        "foundation_only":true,
        "populations":populations,
        "schema_version":3
    })
}

fn population_row(name: &str, ids: &BTreeSet<String>) -> Value {
    json!({"ids":ids.iter().collect::<Vec<_>>(),"name":name,"sha256":population_hash(ids)})
}

fn write_runs(path: &Path, runs: &[RunIdentity]) -> Result<(), String> {
    let mut bytes = Vec::new();
    for run in runs {
        let value = json!({
            "configuration":run.configuration,
            "configuration_preimage":run.configuration_preimage,
            "declared_population":run.declared.iter().collect::<Vec<_>>(),
            "declared_population_sha256":run.declared_hash(),
            "execution_population":run.execution.iter().collect::<Vec<_>>(),
            "execution_population_sha256":run.execution_hash(),
            "logical_run_sha256":run.logical_run_sha256,
            "run_id":run.run_id
        });
        bytes.extend(canonical_json_line(&value)?);
    }
    fs::write(path, bytes).map_err(|error| format!("failed to write '{}': {error}", path.display()))
}

fn generation_fingerprints(validated: &ValidatedCollection) -> Result<Value, String> {
    let corpus_state_sha256 = file_array_hash(
        validated,
        &[
            "manifests/chunking.json",
            "manifests/preprocessing.json",
            "records.jsonl",
        ],
    )?;
    let graph_state_sha256 = file_array_hash(
        validated,
        &["graph-schema.json", "manifests/graph-construction.json"],
    )?;
    let embedding_hash = sha256(&validated.bytes["manifests/embedding.json"]);
    let corpus_embeddings_hash = sha256(&validated.bytes["corpus-embeddings.f32.jsonl"]);
    let normalization_hash = sha256(canonical_json(&normalization_policy())?.as_bytes());
    let quantization_hash = sha256(canonical_json(&quantization_policy())?.as_bytes());
    let bm25_hash = sha256(canonical_json(&bm25_policy())?.as_bytes());
    let mut unique = BTreeMap::new();
    let mut bindings = Vec::new();
    for run in &validated.runs {
        let letter = run.configuration["run_letter"].as_str().unwrap();
        if !matches!(letter, "d" | "e" | "f" | "g") {
            continue;
        }
        let retrieval = if letter == "d" {
            Value::Null
        } else {
            let encoding = run.configuration["vector_encoding"].as_str().unwrap();
            json!({
                "bm25_policy_sha256":if letter=="g"{json!(bm25_hash)}else{Value::Null},
                "files":[
                    {"path":"corpus-embeddings.f32.jsonl","sha256":corpus_embeddings_hash},
                    {"path":"manifests/embedding.json","sha256":embedding_hash}
                ],
                "metric":"cosine",
                "normalization":"unit_l2",
                "normalization_policy_sha256":normalization_hash,
                "quantization_policy_sha256":if matches!(letter,"f"|"g"){json!(quantization_hash)}else{Value::Null},
                "vector_encoding":encoding
            })
        };
        let retrieval_state_sha256 = if retrieval.is_null() {
            Value::Null
        } else {
            json!(sha256(canonical_json(&retrieval)?.as_bytes()))
        };
        let preimage = json!({
            "corpus_id":validated.collection.corpus_id,
            "corpus_state_sha256":corpus_state_sha256,
            "graph_state_sha256":graph_state_sha256,
            "retrieval_state_sha256":retrieval_state_sha256,
            "schema_version":1
        });
        let fingerprint = sha256(canonical_json(&preimage)?.as_bytes());
        unique.entry(fingerprint.clone()).or_insert(preimage);
        bindings.push(json!({"fingerprint":fingerprint,"run_id":run.run_id}));
    }
    bindings.sort_by(|left, right| {
        left["run_id"]
            .as_str()
            .unwrap()
            .cmp(right["run_id"].as_str().unwrap())
    });
    let preimages = unique
        .into_iter()
        .map(|(fingerprint, preimage)| json!({"fingerprint":fingerprint,"preimage":preimage}))
        .collect::<Vec<_>>();
    Ok(json!({"bindings":bindings,"foundation_only":true,"preimages":preimages,"schema_version":1}))
}

fn file_array_hash(validated: &ValidatedCollection, paths: &[&str]) -> Result<String, String> {
    let mut values = paths
        .iter()
        .map(|path| {
            let bytes = validated
                .bytes
                .get(*path)
                .ok_or_else(|| format!("missing generation-state file '{path}'"))?;
            Ok(json!({"path":path,"sha256":sha256(bytes)}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    values.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap()
            .cmp(right["path"].as_str().unwrap())
    });
    Ok(sha256(canonical_json(&Value::Array(values))?.as_bytes()))
}

fn determinism_context() -> Value {
    let environment = json!({
        "cpu_architecture":"synthetic-conformance",
        "cpu_features":[],
        "execution_threads":1,
        "floating_point_mode":"round_to_nearest_ties_to_even",
        "locale":"C",
        "os_build":"synthetic-conformance",
        "runtime_flags":[]
    });
    let environment_preimage = canonical_json(&environment).expect("fixed context is canonical");
    json!({
        "context":{
            "binary_sha256":"cc57e402a8c92ff14601f6390c76b15d1b6a4598e219c8b58009c36e2daa4f97",
            "environment_sha256":sha256(environment_preimage.as_bytes()),
            "runtime_id":"rust",
            "runtime_version":"pinned-conformance",
            "target_triple":"synthetic-conformance"
        },
        "environment":environment,
        "environment_preimage":environment_preimage,
        "foundation_only":true,
        "schema_version":1
    })
}

fn write_foundation_manifest(output: &Path, validated: &ValidatedCollection) -> Result<(), String> {
    let mut files = Vec::new();
    collect_artifact_files(output, output, &mut files)?;
    files.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap()
            .cmp(right["path"].as_str().unwrap())
    });
    write_canonical_json(
        &output.join("foundation-manifest.json"),
        &json!({
            "collection_id":validated.collection.collection_id,
            "collection_version":validated.collection.collection_version,
            "files":files,
            "foundation_only":true,
            "normative_fixture_sha256":"4d7b920b8ae591f0c05cd41abbc36c50210bbf23e6bfa0e09b4eebbffdea4f46",
            "run_count":validated.runs.len(),
            "schema_version":1
        }),
    )
}

fn collect_artifact_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<Value>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("failed to read '{}': {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read artifact entry: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            collect_artifact_files(root, &entry.path(), files)?;
        } else {
            let bytes = fs::read(entry.path())
                .map_err(|error| format!("failed to read '{}': {error}", entry.path().display()))?;
            let path = entry
                .path()
                .strip_prefix(root)
                .expect("artifact is beneath root")
                .to_str()
                .ok_or_else(|| "artifact path is not UTF-8".to_owned())?
                .to_owned();
            files.push(json!({"bytes":bytes.len(),"path":path,"sha256":sha256(&bytes)}));
        }
    }
    Ok(())
}

fn compare_directories(first: &Path, second: &Path) -> Result<(), String> {
    let mut first_files = BTreeMap::new();
    collect_paths(first, first, &mut first_files)?;
    let mut second_files = BTreeMap::new();
    collect_paths(second, second, &mut second_files)?;
    if first_files.keys().collect::<Vec<_>>() != second_files.keys().collect::<Vec<_>>() {
        return Err(format!(
            "foundation rerun file sets differ: first {:?}, second {:?}",
            first_files.keys().collect::<Vec<_>>(),
            second_files.keys().collect::<Vec<_>>()
        ));
    }
    for (path, first_bytes) in first_files {
        let second_bytes = &second_files[&path];
        if first_bytes != *second_bytes {
            return Err(format!(
                "foundation rerun first differing file '{}' at byte offset {}",
                path,
                first_difference(&first_bytes, second_bytes)
            ));
        }
    }
    Ok(())
}

fn collect_paths(
    root: &Path,
    directory: &Path,
    output: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("failed to read '{}': {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read entry: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            collect_paths(root, &entry.path(), output)?;
        } else {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("artifact is beneath root")
                .to_str()
                .ok_or_else(|| "artifact path is not UTF-8".to_owned())?
                .to_owned();
            output.insert(
                relative,
                fs::read(&path)
                    .map_err(|error| format!("failed to read '{}': {error}", path.display()))?,
            );
        }
    }
    Ok(())
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
                "failed to create temporary directory '{}': {error}",
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

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    #[test]
    fn foundation_rerun_is_byte_identical() {
        verify_deterministic_rerun(&fixture_root()).unwrap();
    }

    #[test]
    fn rerun_comparator_reports_first_file_and_byte_offset() {
        let first = TemporaryDirectory::new("vectorkit-v3-compare-test-a").unwrap();
        let second = TemporaryDirectory::new("vectorkit-v3-compare-test-b").unwrap();
        fs::write(first.path.join("a.json"), b"same\n").unwrap();
        fs::write(second.path.join("a.json"), b"same\n").unwrap();
        fs::write(first.path.join("b.json"), b"abcde\n").unwrap();
        fs::write(second.path.join("b.json"), b"abXde\n").unwrap();
        let error = compare_directories(&first.path, &second.path).unwrap_err();
        assert_eq!(
            error,
            "foundation rerun first differing file 'b.json' at byte offset 2"
        );
    }
}
