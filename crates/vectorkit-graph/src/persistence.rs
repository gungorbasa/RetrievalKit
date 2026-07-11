use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vectorkit_core::{CorpusId, ExactVectorIndex, GenerationId};

use crate::error::{GraphError, Result};
use crate::{GraphIndex, GraphSnapshotPayload, SchemaHash};

const FORMAT_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";
const SNAPSHOTS_DIRECTORY: &str = ".snapshots";
const CORE_DIRECTORY: &str = "core";
const SCHEMA_FILE: &str = "schema.json";
const GRAPH_FILE: &str = "graph.bin";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphDatabaseFileSizes {
    pub schema_bytes: u64,
    pub graph_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format_version: u32,
    snapshot_id: String,
    corpus_id: CorpusId,
    generation: GenerationId,
    schema_hash: SchemaHash,
    schema_bytes: u64,
    graph_bytes: u64,
    checksums: Checksums,
}

#[derive(Debug, Serialize, Deserialize)]
struct Checksums {
    algorithm: String,
    schema: String,
    graph: String,
}

pub(crate) fn save(index: &GraphIndex, directory: &Path) -> Result<GraphDatabaseFileSizes> {
    fs::create_dir_all(directory)
        .map_err(|error| io_error("create graph database directory", directory, error))?;
    let snapshots = directory.join(SNAPSHOTS_DIRECTORY);
    fs::create_dir_all(&snapshots)
        .map_err(|error| io_error("create graph snapshots directory", &snapshots, error))?;

    let snapshot_id = next_snapshot_id(index.core.generation())?;
    let staging = snapshots.join(format!(".staging-{snapshot_id}"));
    let published = snapshots.join(&snapshot_id);
    fs::create_dir(&staging)
        .map_err(|error| io_error("create staged graph snapshot", &staging, error))?;

    let result = stage_and_publish(
        index,
        directory,
        &snapshots,
        &staging,
        &published,
        snapshot_id,
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn stage_and_publish(
    index: &GraphIndex,
    directory: &Path,
    snapshots: &Path,
    staging: &Path,
    published: &Path,
    snapshot_id: String,
) -> Result<GraphDatabaseFileSizes> {
    let core_directory = staging.join(CORE_DIRECTORY);
    index.core.save_to_dir(&core_directory)?;

    let payload = index.snapshot_payload()?;
    let schema_path = staging.join(SCHEMA_FILE);
    let graph_path = staging.join(GRAPH_FILE);
    write_synced(&schema_path, &payload.schema_bytes)?;
    write_synced(&graph_path, &payload.graph_bytes)?;

    let sizes = GraphDatabaseFileSizes {
        schema_bytes: byte_len(&payload.schema_bytes, "schema")?,
        graph_bytes: byte_len(&payload.graph_bytes, "graph")?,
    };
    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        snapshot_id: snapshot_id.clone(),
        corpus_id: index.core.corpus_id().clone(),
        generation: index.core.generation(),
        schema_hash: payload.schema_hash,
        schema_bytes: sizes.schema_bytes,
        graph_bytes: sizes.graph_bytes,
        checksums: Checksums {
            algorithm: "blake3".to_owned(),
            schema: checksum(&payload.schema_bytes),
            graph: checksum(&payload.graph_bytes),
        },
    };

    validate_manifest(&manifest)?;
    validate_generation(staging, &manifest)?;
    sync_directory(staging)?;
    fs::rename(staging, published)
        .map_err(|error| io_error("publish graph snapshot", published, error))?;
    sync_directory(snapshots)?;

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| invalid_snapshot(format!("could not encode graph manifest: {error}")))?;
    let manifest_path = directory.join(MANIFEST_FILE);
    let temporary_manifest = directory.join(format!("manifest.{snapshot_id}.tmp"));
    write_synced(&temporary_manifest, &manifest_bytes)?;
    fs::rename(&temporary_manifest, &manifest_path)
        .map_err(|error| io_error("activate graph manifest", &manifest_path, error))?;
    sync_directory(directory)?;
    Ok(sizes)
}

pub(crate) fn load(directory: &Path) -> Result<GraphIndex> {
    let manifest = read_manifest(directory)?;
    let generation = generation_directory(directory, &manifest);
    load_generation(&generation, &manifest)
}

pub(crate) fn validate(directory: &Path) -> Result<()> {
    load(directory).map(|_| ())
}

fn read_manifest(directory: &Path) -> Result<Manifest> {
    let path = directory.join(MANIFEST_FILE);
    let bytes = fs::read(&path).map_err(|error| io_error("read graph manifest", &path, error))?;
    let manifest = serde_json::from_slice(&bytes)
        .map_err(|error| invalid_snapshot(format!("could not decode graph manifest: {error}")))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.format_version != FORMAT_VERSION {
        return Err(invalid_snapshot(format!(
            "unsupported graph database format version {}",
            manifest.format_version
        )));
    }
    if !safe_snapshot_id(&manifest.snapshot_id) {
        return Err(invalid_snapshot(
            "manifest snapshot ID is not a safe path component",
        ));
    }
    if manifest.checksums.algorithm != "blake3" {
        return Err(invalid_snapshot(
            "manifest checksum algorithm must be blake3",
        ));
    }
    for (label, value) in [
        ("schema", manifest.checksums.schema.as_str()),
        ("graph", manifest.checksums.graph.as_str()),
    ] {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(invalid_snapshot(format!(
                "manifest {label} checksum must be 64 lowercase hexadecimal characters"
            )));
        }
    }
    Ok(())
}

fn validate_generation(directory: &Path, manifest: &Manifest) -> Result<()> {
    load_generation(directory, manifest).map(|_| ())
}

fn load_generation(directory: &Path, manifest: &Manifest) -> Result<GraphIndex> {
    let schema_path = directory.join(SCHEMA_FILE);
    let graph_path = directory.join(GRAPH_FILE);
    let schema_bytes = read_exact_payload(&schema_path, manifest.schema_bytes, "schema")?;
    let graph_bytes = read_exact_payload(&graph_path, manifest.graph_bytes, "graph")?;
    verify_checksum("schema", &schema_bytes, &manifest.checksums.schema)?;
    verify_checksum("graph", &graph_bytes, &manifest.checksums.graph)?;

    let core_directory = directory.join(CORE_DIRECTORY);
    let core = ExactVectorIndex::load_from_dir(&core_directory)?;
    if core.corpus_id() != &manifest.corpus_id || core.generation() != manifest.generation {
        return Err(invalid_snapshot(
            "core corpus or generation does not match the composite manifest",
        ));
    }
    let payload = GraphSnapshotPayload {
        schema_bytes,
        graph_bytes,
        schema_hash: manifest.schema_hash,
    };
    GraphIndex::from_snapshot_payload(core, &payload)
}

fn generation_directory(directory: &Path, manifest: &Manifest) -> PathBuf {
    directory
        .join(SNAPSHOTS_DIRECTORY)
        .join(&manifest.snapshot_id)
}

fn read_exact_payload(path: &Path, expected: u64, label: &str) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)
        .map_err(|error| io_error(&format!("inspect graph {label} payload"), path, error))?;
    if metadata.len() != expected {
        return Err(invalid_snapshot(format!(
            "graph {label} payload size {} does not match manifest {expected}",
            metadata.len()
        )));
    }
    fs::read(path).map_err(|error| io_error(&format!("read graph {label} payload"), path, error))
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path).map_err(|error| io_error("create file", path, error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("write file", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync file", path, error))
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync directory", path, error))
}

fn verify_checksum(label: &str, bytes: &[u8], expected: &str) -> Result<()> {
    let actual = checksum(bytes);
    if actual != expected {
        return Err(invalid_snapshot(format!(
            "graph {label} checksum mismatch; restore or rebuild the database"
        )));
    }
    Ok(())
}

fn checksum(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn byte_len(bytes: &[u8], label: &str) -> Result<u64> {
    u64::try_from(bytes.len())
        .map_err(|_| invalid_snapshot(format!("graph {label} payload exceeds u64")))
}

fn next_snapshot_id(generation: GenerationId) -> Result<String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| invalid_snapshot(format!("system clock precedes Unix epoch: {error}")))?
        .as_nanos();
    Ok(format!(
        "g{}-{nanos}-{}",
        generation.get(),
        std::process::id()
    ))
}

fn safe_snapshot_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> GraphError {
    invalid_snapshot(format!("could not {action} '{}': {error}", path.display()))
}

fn invalid_snapshot(message: impl Into<String>) -> GraphError {
    GraphError::InvalidSnapshot {
        message: message.into(),
    }
}
