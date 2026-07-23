use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use retrievalkit_core::{CorpusId, CorpusIndex, ExactVectorIndex, GenerationId};

use crate::error::{GraphError, Result};
use crate::{
    GraphDatabase, GraphEngine, GraphIndex, GraphRetrievalDatabase, GraphSnapshotPayload,
    SchemaHash,
};

const FORMAT_VERSION: u32 = 1;
const CAPABILITY_FORMAT_VERSION: u32 = 2;
const MANIFEST_FILE: &str = "manifest.json";
const SNAPSHOTS_DIRECTORY: &str = ".snapshots";
const CORE_DIRECTORY: &str = "core";
const CORPUS_DIRECTORY: &str = "corpus";
const CORPUS_FILE: &str = "corpus.bin";
const GRAPH_DIRECTORY: &str = "graph";
const RETRIEVAL_DIRECTORY: &str = "retrieval";
const SCHEMA_FILE: &str = "schema.json";
const GRAPH_FILE: &str = "graph.bin";
const WRITER_LOCK_FILE: &str = ".writer.lock";
const GENERATION_LEASE_FILE: &str = ".lease";

#[derive(Debug)]
pub(crate) struct GenerationLease {
    file: File,
}

impl Drop for GenerationLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

struct DatabaseLock {
    file: File,
}

impl DatabaseLock {
    fn acquire_shared(directory: &Path) -> Result<Self> {
        let path = directory.join(WRITER_LOCK_FILE);
        let file = open_lock_file(&path)?;
        file.lock_shared()
            .map_err(|error| io_error("acquire graph database open lock", &path, error))?;
        Ok(Self { file })
    }

    fn try_acquire_writer(directory: &Path) -> Result<Self> {
        let path = directory.join(WRITER_LOCK_FILE);
        let file = open_lock_file(&path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Self { file }),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                Err(GraphError::WriterBusy {
                    path: directory.display().to_string(),
                })
            }
            Err(error) => Err(io_error("acquire graph database writer lock", &path, error)),
        }
    }
}

impl Drop for DatabaseLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphDatabaseFileSizes {
    pub corpus_bytes: u64,
    pub schema_bytes: u64,
    pub graph_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveCheckpoint {
    StagingCreated,
    CoreWritten,
    SchemaWritten,
    GraphWritten,
    StagingValidated,
    GenerationPublished,
    ManifestWritten,
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

#[derive(Debug, Serialize, Deserialize)]
struct GraphOnlyManifest {
    format_version: u32,
    capability: String,
    snapshot_id: String,
    corpus_id: CorpusId,
    generation: GenerationId,
    schema_hash: SchemaHash,
    corpus_bytes: u64,
    schema_bytes: u64,
    graph_bytes: u64,
    checksums: GraphOnlyChecksums,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphOnlyChecksums {
    algorithm: String,
    corpus: String,
    schema: String,
    graph: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphRetrievalManifest {
    format_version: u32,
    capability: String,
    snapshot_id: String,
    corpus_id: CorpusId,
    generation: GenerationId,
    schema_hash: SchemaHash,
    schema_bytes: u64,
    graph_bytes: u64,
    checksums: Checksums,
}

pub(crate) fn save(index: &GraphIndex, directory: &Path) -> Result<GraphDatabaseFileSizes> {
    save_with_checkpoints(index, directory, |_| Ok(()))
}

fn save_with_checkpoints(
    index: &GraphIndex,
    directory: &Path,
    mut checkpoint: impl FnMut(SaveCheckpoint) -> Result<()>,
) -> Result<GraphDatabaseFileSizes> {
    fs::create_dir_all(directory)
        .map_err(|error| io_error("create graph database directory", directory, error))?;
    let _writer_lock = DatabaseLock::try_acquire_writer(directory)?;
    let snapshots = directory.join(SNAPSHOTS_DIRECTORY);
    fs::create_dir_all(&snapshots)
        .map_err(|error| io_error("create graph snapshots directory", &snapshots, error))?;
    recover_snapshots(directory, &snapshots)?;
    cleanup_temporary_manifests(directory);

    let snapshot_id = next_snapshot_id(index.core.generation())?;
    let staging = snapshots.join(format!(".staging-{snapshot_id}"));
    let published = snapshots.join(&snapshot_id);
    fs::create_dir(&staging)
        .map_err(|error| io_error("create staged graph snapshot", &staging, error))?;
    write_synced(&staging.join(GENERATION_LEASE_FILE), &[])?;
    let result = (|| {
        checkpoint(SaveCheckpoint::StagingCreated)?;
        stage_and_publish(
            index,
            directory,
            &snapshots,
            &staging,
            &published,
            snapshot_id,
            &mut checkpoint,
        )
    })();
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
    checkpoint: &mut impl FnMut(SaveCheckpoint) -> Result<()>,
) -> Result<GraphDatabaseFileSizes> {
    let core_directory = staging.join(CORE_DIRECTORY);
    index.core.save_to_dir(&core_directory)?;
    checkpoint(SaveCheckpoint::CoreWritten)?;

    let payload = index.snapshot_payload()?;
    let schema_path = staging.join(SCHEMA_FILE);
    let graph_path = staging.join(GRAPH_FILE);
    write_synced(&schema_path, &payload.schema_bytes)?;
    checkpoint(SaveCheckpoint::SchemaWritten)?;
    write_synced(&graph_path, &payload.graph_bytes)?;
    checkpoint(SaveCheckpoint::GraphWritten)?;

    let sizes = GraphDatabaseFileSizes {
        corpus_bytes: 0,
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
    checkpoint(SaveCheckpoint::StagingValidated)?;
    fs::rename(staging, published)
        .map_err(|error| io_error("publish graph snapshot", published, error))?;
    sync_directory(snapshots)?;
    checkpoint(SaveCheckpoint::GenerationPublished)?;

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| invalid_snapshot(format!("could not encode graph manifest: {error}")))?;
    let manifest_path = directory.join(MANIFEST_FILE);
    let temporary_manifest = directory.join(format!("manifest.{snapshot_id}.tmp"));
    write_synced(&temporary_manifest, &manifest_bytes)?;
    checkpoint(SaveCheckpoint::ManifestWritten)?;
    fs::rename(&temporary_manifest, &manifest_path)
        .map_err(|error| io_error("activate graph manifest", &manifest_path, error))?;
    sync_directory(directory)?;
    cleanup_unreferenced_snapshots(snapshots, &snapshot_id);
    Ok(sizes)
}

pub(crate) fn save_graph_database(
    database: &GraphDatabase,
    directory: &Path,
) -> Result<GraphDatabaseFileSizes> {
    fs::create_dir_all(directory)
        .map_err(|error| io_error("create graph database directory", directory, error))?;
    let _writer_lock = DatabaseLock::try_acquire_writer(directory)?;
    let snapshots = directory.join(SNAPSHOTS_DIRECTORY);
    fs::create_dir_all(&snapshots)
        .map_err(|error| io_error("create graph snapshots directory", &snapshots, error))?;
    recover_graph_only_snapshots(directory, &snapshots)?;
    cleanup_temporary_manifests(directory);

    let snapshot_id = next_snapshot_id(database.corpus.generation())?;
    let staging = snapshots.join(format!(".staging-{snapshot_id}"));
    let published = snapshots.join(&snapshot_id);
    fs::create_dir(&staging)
        .map_err(|error| io_error("create staged graph snapshot", &staging, error))?;
    write_synced(&staging.join(GENERATION_LEASE_FILE), &[])?;

    let result = (|| {
        let corpus_directory = staging.join(CORPUS_DIRECTORY);
        let graph_directory = staging.join(GRAPH_DIRECTORY);
        fs::create_dir(&corpus_directory).map_err(|error| {
            io_error("create corpus payload directory", &corpus_directory, error)
        })?;
        fs::create_dir(&graph_directory)
            .map_err(|error| io_error("create graph payload directory", &graph_directory, error))?;

        let corpus_bytes = database.corpus.snapshot_bytes()?;
        let payload = database.graph.snapshot_payload(&database.corpus)?;
        write_synced(&corpus_directory.join(CORPUS_FILE), &corpus_bytes)?;
        write_synced(&graph_directory.join(SCHEMA_FILE), &payload.schema_bytes)?;
        write_synced(&graph_directory.join(GRAPH_FILE), &payload.graph_bytes)?;

        let sizes = GraphDatabaseFileSizes {
            corpus_bytes: byte_len(&corpus_bytes, "corpus")?,
            schema_bytes: byte_len(&payload.schema_bytes, "schema")?,
            graph_bytes: byte_len(&payload.graph_bytes, "graph")?,
        };
        let manifest = GraphOnlyManifest {
            format_version: CAPABILITY_FORMAT_VERSION,
            capability: "graph".to_owned(),
            snapshot_id: snapshot_id.clone(),
            corpus_id: database.corpus.corpus_id().clone(),
            generation: database.corpus.generation(),
            schema_hash: payload.schema_hash,
            corpus_bytes: sizes.corpus_bytes,
            schema_bytes: sizes.schema_bytes,
            graph_bytes: sizes.graph_bytes,
            checksums: GraphOnlyChecksums {
                algorithm: "blake3".to_owned(),
                corpus: checksum(&corpus_bytes),
                schema: checksum(&payload.schema_bytes),
                graph: checksum(&payload.graph_bytes),
            },
        };
        validate_graph_only_manifest(&manifest)?;
        load_graph_only_generation(&staging, &manifest)?;
        sync_directory(&corpus_directory)?;
        sync_directory(&graph_directory)?;
        sync_directory(&staging)?;
        fs::rename(&staging, &published)
            .map_err(|error| io_error("publish graph snapshot", &published, error))?;
        sync_directory(&snapshots)?;

        let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            invalid_snapshot(format!("could not encode graph manifest: {error}"))
        })?;
        let manifest_path = directory.join(MANIFEST_FILE);
        let temporary_manifest = directory.join(format!("manifest.{snapshot_id}.tmp"));
        write_synced(&temporary_manifest, &manifest_bytes)?;
        fs::rename(&temporary_manifest, &manifest_path)
            .map_err(|error| io_error("activate graph manifest", &manifest_path, error))?;
        sync_directory(directory)?;
        cleanup_unreferenced_snapshots(&snapshots, &snapshot_id);
        Ok(sizes)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub(crate) fn load_graph_database(directory: &Path) -> Result<GraphDatabase> {
    let open_lock = DatabaseLock::acquire_shared(directory)?;
    let manifest = read_graph_only_manifest(directory)?;
    let generation = directory
        .join(SNAPSHOTS_DIRECTORY)
        .join(&manifest.snapshot_id);
    let lease = acquire_generation_lease(&generation)?;
    drop(open_lock);
    let mut database = load_graph_only_generation(&generation, &manifest)?;
    database._generation_lease = Some(lease);
    Ok(database)
}

pub(crate) fn validate_graph_database(directory: &Path) -> Result<()> {
    load_graph_database(directory).map(|_| ())
}

fn read_graph_only_manifest(directory: &Path) -> Result<GraphOnlyManifest> {
    let path = directory.join(MANIFEST_FILE);
    let bytes = fs::read(&path).map_err(|error| io_error("read graph manifest", &path, error))?;
    let manifest = serde_json::from_slice(&bytes)
        .map_err(|error| invalid_snapshot(format!("could not decode graph manifest: {error}")))?;
    validate_graph_only_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_graph_only_manifest(manifest: &GraphOnlyManifest) -> Result<()> {
    if manifest.format_version != CAPABILITY_FORMAT_VERSION || manifest.capability != "graph" {
        return Err(GraphError::IncompatibleVersion {
            message: format!(
                "unsupported graph database capability format version {} ({})",
                manifest.format_version, manifest.capability
            ),
        });
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
        ("corpus", manifest.checksums.corpus.as_str()),
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

fn load_graph_only_generation(
    directory: &Path,
    manifest: &GraphOnlyManifest,
) -> Result<GraphDatabase> {
    let corpus_path = directory.join(CORPUS_DIRECTORY).join(CORPUS_FILE);
    let graph_directory = directory.join(GRAPH_DIRECTORY);
    let schema_path = graph_directory.join(SCHEMA_FILE);
    let graph_path = graph_directory.join(GRAPH_FILE);
    let corpus_bytes = read_exact_payload(&corpus_path, manifest.corpus_bytes, "corpus")?;
    let schema_bytes = read_exact_payload(&schema_path, manifest.schema_bytes, "schema")?;
    let graph_bytes = read_exact_payload(&graph_path, manifest.graph_bytes, "graph")?;
    verify_checksum("corpus", &corpus_bytes, &manifest.checksums.corpus)?;
    verify_checksum("schema", &schema_bytes, &manifest.checksums.schema)?;
    verify_checksum("graph", &graph_bytes, &manifest.checksums.graph)?;

    let corpus = CorpusIndex::from_snapshot_bytes(&corpus_bytes)?;
    if corpus.corpus_id() != &manifest.corpus_id || corpus.generation() != manifest.generation {
        return Err(invalid_snapshot(
            "corpus identity or generation does not match the graph manifest",
        ));
    }
    let payload = GraphSnapshotPayload {
        schema_bytes,
        graph_bytes,
        schema_hash: manifest.schema_hash,
    };
    let graph = GraphEngine::from_snapshot_payload(&corpus, &payload)?;
    Ok(GraphDatabase {
        corpus,
        graph,
        _generation_lease: None,
    })
}

fn recover_graph_only_snapshots(directory: &Path, snapshots: &Path) -> Result<()> {
    let manifest_path = directory.join(MANIFEST_FILE);
    let active = if manifest_path.exists() {
        Some(read_graph_only_manifest(directory)?.snapshot_id)
    } else {
        None
    };
    let entries = fs::read_dir(snapshots)
        .map_err(|error| io_error("list graph snapshots", snapshots, error))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| io_error("read graph snapshot entry", snapshots, error))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".staging-") {
            let _ = fs::remove_dir_all(entry.path());
        } else if active.as_deref() != Some(name.as_ref()) {
            try_remove_unleased_generation(&entry.path());
        }
    }
    Ok(())
}

pub(crate) fn save_graph_retrieval_database(
    database: &GraphRetrievalDatabase,
    directory: &Path,
) -> Result<GraphDatabaseFileSizes> {
    fs::create_dir_all(directory).map_err(|error| {
        io_error(
            "create graph retrieval database directory",
            directory,
            error,
        )
    })?;
    let _writer_lock = DatabaseLock::try_acquire_writer(directory)?;
    let snapshots = directory.join(SNAPSHOTS_DIRECTORY);
    fs::create_dir_all(&snapshots)
        .map_err(|error| io_error("create graph snapshots directory", &snapshots, error))?;
    recover_graph_retrieval_snapshots(directory, &snapshots)?;
    cleanup_temporary_manifests(directory);

    let snapshot_id = next_snapshot_id(database.corpus().generation())?;
    let staging = snapshots.join(format!(".staging-{snapshot_id}"));
    let published = snapshots.join(&snapshot_id);
    fs::create_dir(&staging)
        .map_err(|error| io_error("create staged graph snapshot", &staging, error))?;
    write_synced(&staging.join(GENERATION_LEASE_FILE), &[])?;

    let result = (|| {
        let retrieval_directory = staging.join(RETRIEVAL_DIRECTORY);
        let graph_directory = staging.join(GRAPH_DIRECTORY);
        database.retrieval.save_to_dir(&retrieval_directory)?;
        fs::create_dir(&graph_directory)
            .map_err(|error| io_error("create graph payload directory", &graph_directory, error))?;
        let payload = database.graph.snapshot_payload(database.corpus())?;
        write_synced(&graph_directory.join(SCHEMA_FILE), &payload.schema_bytes)?;
        write_synced(&graph_directory.join(GRAPH_FILE), &payload.graph_bytes)?;

        let sizes = GraphDatabaseFileSizes {
            corpus_bytes: 0,
            schema_bytes: byte_len(&payload.schema_bytes, "schema")?,
            graph_bytes: byte_len(&payload.graph_bytes, "graph")?,
        };
        let manifest = GraphRetrievalManifest {
            format_version: CAPABILITY_FORMAT_VERSION,
            capability: "graph_retrieval".to_owned(),
            snapshot_id: snapshot_id.clone(),
            corpus_id: database.corpus().corpus_id().clone(),
            generation: database.corpus().generation(),
            schema_hash: payload.schema_hash,
            schema_bytes: sizes.schema_bytes,
            graph_bytes: sizes.graph_bytes,
            checksums: Checksums {
                algorithm: "blake3".to_owned(),
                schema: checksum(&payload.schema_bytes),
                graph: checksum(&payload.graph_bytes),
            },
        };
        validate_graph_retrieval_manifest(&manifest)?;
        load_graph_retrieval_generation(&staging, &manifest)?;
        sync_directory(&retrieval_directory)?;
        sync_directory(&graph_directory)?;
        sync_directory(&staging)?;
        fs::rename(&staging, &published)
            .map_err(|error| io_error("publish graph snapshot", &published, error))?;
        sync_directory(&snapshots)?;

        let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            invalid_snapshot(format!("could not encode graph manifest: {error}"))
        })?;
        let manifest_path = directory.join(MANIFEST_FILE);
        let temporary_manifest = directory.join(format!("manifest.{snapshot_id}.tmp"));
        write_synced(&temporary_manifest, &manifest_bytes)?;
        fs::rename(&temporary_manifest, &manifest_path)
            .map_err(|error| io_error("activate graph manifest", &manifest_path, error))?;
        sync_directory(directory)?;
        cleanup_unreferenced_snapshots(&snapshots, &snapshot_id);
        Ok(sizes)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub(crate) fn load_graph_retrieval_database(directory: &Path) -> Result<GraphRetrievalDatabase> {
    let open_lock = DatabaseLock::acquire_shared(directory)?;
    let manifest = read_graph_retrieval_manifest(directory)?;
    let generation = directory
        .join(SNAPSHOTS_DIRECTORY)
        .join(&manifest.snapshot_id);
    let lease = acquire_generation_lease(&generation)?;
    drop(open_lock);
    let mut database = load_graph_retrieval_generation(&generation, &manifest)?;
    database._generation_lease = Some(lease);
    Ok(database)
}

pub(crate) fn validate_graph_retrieval_database(directory: &Path) -> Result<()> {
    load_graph_retrieval_database(directory).map(|_| ())
}

fn read_graph_retrieval_manifest(directory: &Path) -> Result<GraphRetrievalManifest> {
    let path = directory.join(MANIFEST_FILE);
    let bytes = fs::read(&path).map_err(|error| io_error("read graph manifest", &path, error))?;
    let manifest = serde_json::from_slice(&bytes)
        .map_err(|error| invalid_snapshot(format!("could not decode graph manifest: {error}")))?;
    validate_graph_retrieval_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_graph_retrieval_manifest(manifest: &GraphRetrievalManifest) -> Result<()> {
    if manifest.format_version != CAPABILITY_FORMAT_VERSION
        || manifest.capability != "graph_retrieval"
    {
        return Err(GraphError::IncompatibleVersion {
            message: format!(
                "unsupported graph retrieval capability format version {} ({})",
                manifest.format_version, manifest.capability
            ),
        });
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

fn load_graph_retrieval_generation(
    directory: &Path,
    manifest: &GraphRetrievalManifest,
) -> Result<GraphRetrievalDatabase> {
    let retrieval =
        retrievalkit_core::RetrievalDatabase::load_from_dir(directory.join(RETRIEVAL_DIRECTORY))?;
    if retrieval.corpus().corpus_id() != &manifest.corpus_id
        || retrieval.corpus().generation() != manifest.generation
    {
        return Err(invalid_snapshot(
            "retrieval corpus identity or generation does not match the graph manifest",
        ));
    }
    let graph_directory = directory.join(GRAPH_DIRECTORY);
    let schema_bytes = read_exact_payload(
        &graph_directory.join(SCHEMA_FILE),
        manifest.schema_bytes,
        "schema",
    )?;
    let graph_bytes = read_exact_payload(
        &graph_directory.join(GRAPH_FILE),
        manifest.graph_bytes,
        "graph",
    )?;
    verify_checksum("schema", &schema_bytes, &manifest.checksums.schema)?;
    verify_checksum("graph", &graph_bytes, &manifest.checksums.graph)?;
    let graph = GraphEngine::from_snapshot_payload(
        retrieval.corpus(),
        &GraphSnapshotPayload {
            schema_bytes,
            graph_bytes,
            schema_hash: manifest.schema_hash,
        },
    )?;
    Ok(GraphRetrievalDatabase {
        retrieval,
        graph,
        _generation_lease: None,
    })
}

fn recover_graph_retrieval_snapshots(directory: &Path, snapshots: &Path) -> Result<()> {
    let manifest_path = directory.join(MANIFEST_FILE);
    let active = if manifest_path.exists() {
        Some(read_graph_retrieval_manifest(directory)?.snapshot_id)
    } else {
        None
    };
    let entries = fs::read_dir(snapshots)
        .map_err(|error| io_error("list graph snapshots", snapshots, error))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| io_error("read graph snapshot entry", snapshots, error))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".staging-") {
            let _ = fs::remove_dir_all(entry.path());
        } else if active.as_deref() != Some(name.as_ref()) {
            try_remove_unleased_generation(&entry.path());
        }
    }
    Ok(())
}

pub(crate) fn load(directory: &Path) -> Result<GraphIndex> {
    let open_lock = DatabaseLock::acquire_shared(directory)?;
    let manifest = read_manifest(directory)?;
    let generation = generation_directory(directory, &manifest);
    let lease = acquire_generation_lease(&generation)?;
    drop(open_lock);
    let mut index = load_generation(&generation, &manifest)?;
    index._generation_lease = Some(lease);
    Ok(index)
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
        return Err(GraphError::IncompatibleVersion {
            message: format!(
                "unsupported graph database format version {}",
                manifest.format_version
            ),
        });
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

fn acquire_generation_lease(directory: &Path) -> Result<GenerationLease> {
    let path = directory.join(GENERATION_LEASE_FILE);
    let file = open_lock_file(&path)?;
    file.lock_shared()
        .map_err(|error| io_error("acquire graph generation lease", &path, error))?;
    Ok(GenerationLease { file })
}

fn recover_snapshots(directory: &Path, snapshots: &Path) -> Result<()> {
    let manifest_path = directory.join(MANIFEST_FILE);
    let active = if manifest_path.exists() {
        Some(read_manifest(directory)?.snapshot_id)
    } else {
        None
    };
    let entries = fs::read_dir(snapshots)
        .map_err(|error| io_error("list graph snapshots", snapshots, error))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| io_error("read graph snapshot entry", snapshots, error))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".staging-") {
            let _ = fs::remove_dir_all(entry.path());
        } else if active.as_deref() != Some(name.as_ref()) {
            try_remove_unleased_generation(&entry.path());
        }
    }
    Ok(())
}

fn cleanup_unreferenced_snapshots(snapshots: &Path, active: &str) {
    let Ok(entries) = fs::read_dir(snapshots) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() != active {
            try_remove_unleased_generation(&entry.path());
        }
    }
}

fn cleanup_temporary_manifests(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("manifest.") && name.ends_with(".tmp") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn try_remove_unleased_generation(directory: &Path) {
    if !directory.is_dir() {
        return;
    }
    let path = directory.join(GENERATION_LEASE_FILE);
    let Ok(file) = open_lock_file(&path) else {
        return;
    };
    if file.try_lock_exclusive().is_ok() {
        let _ = fs::remove_dir_all(directory);
        let _ = FileExt::unlock(&file);
    }
}

fn open_lock_file(path: &Path) -> Result<File> {
    File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| io_error("open lock file", path, error))
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use retrievalkit_core::{
        ChunkKey, ExactVectorIndex, IndexConfig, Metadata, Record, RecordChunkInput, RecordId,
        RecordType, VectorEncoding, VectorMetric,
    };

    use super::{save_with_checkpoints, SaveCheckpoint, MANIFEST_FILE, SNAPSHOTS_DIRECTORY};
    use crate::{GraphError, GraphIndex, GraphSchema, NodeType, RecordNodeSchema};

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "retrievalkit-graph-faults-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn failure_at_every_pre_activation_checkpoint_preserves_active_snapshot() {
        let directory = TestDirectory::new();
        let graph = fixture();
        graph.save_to_dir(&directory.0).unwrap();
        let original_manifest = std::fs::read(directory.0.join(MANIFEST_FILE)).unwrap();
        let checkpoints = [
            SaveCheckpoint::StagingCreated,
            SaveCheckpoint::CoreWritten,
            SaveCheckpoint::SchemaWritten,
            SaveCheckpoint::GraphWritten,
            SaveCheckpoint::StagingValidated,
            SaveCheckpoint::GenerationPublished,
            SaveCheckpoint::ManifestWritten,
        ];

        for target in checkpoints {
            let error = save_with_checkpoints(&graph, &directory.0, |checkpoint| {
                if checkpoint == target {
                    Err(GraphError::InvalidSnapshot {
                        message: format!("injected failure at {checkpoint:?}"),
                    })
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            assert!(matches!(error, GraphError::InvalidSnapshot { .. }));
            assert_eq!(
                std::fs::read(directory.0.join(MANIFEST_FILE)).unwrap(),
                original_manifest
            );
            GraphIndex::validate_dir(&directory.0).unwrap();
        }

        graph.save_to_dir(&directory.0).unwrap();
        GraphIndex::validate_dir(&directory.0).unwrap();
        let published = std::fs::read_dir(directory.0.join(SNAPSHOTS_DIRECTORY))
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .count();
        assert_eq!(published, 1);
    }

    fn fixture() -> GraphIndex {
        let config =
            IndexConfig::new(2, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
        let mut core = ExactVectorIndex::try_with_config(config).unwrap();
        core.upsert_record(
            Record {
                id: RecordId::new("item").unwrap(),
                record_type: RecordType::new("Item").unwrap(),
                fields: BTreeMap::new(),
                content: None,
            },
            Metadata::new(),
            vec![RecordChunkInput {
                key: ChunkKey::new("body").unwrap(),
                text: "item".to_owned(),
                embedding: vec![1.0, 0.0],
                metadata: Metadata::new(),
            }],
        )
        .unwrap();
        let schema = GraphSchema::new(vec![RecordNodeSchema {
            record_type: RecordType::new("Item").unwrap(),
            node_type: NodeType::new("Item").unwrap(),
            queryable_fields: vec![],
        }]);
        GraphIndex::build(core, schema).unwrap()
    }
}
