mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde_json::Value;
use vectorkit_graph::{GraphError, GraphIndex, GraphQuery, GraphScalar, Seed};

use common::{field, node_type, social_core, social_schema};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vectorkit-graph-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn query() -> GraphQuery {
    GraphQuery::new(Seed::Equals {
        node_type: node_type("Person"),
        field: vectorkit_graph::FieldPath::single(field("name")),
        values: vec![GraphScalar::String("Alice".to_owned())],
    })
}

fn active_generation(directory: &Path) -> PathBuf {
    let manifest: Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    directory
        .join(".snapshots")
        .join(manifest["snapshot_id"].as_str().unwrap())
}

#[test]
fn composite_save_load_and_read_only_validation_round_trip() {
    let directory = TestDirectory::new("round-trip");
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    let expected = graph.graph_query(&query(), None).unwrap();
    let sizes = graph.save_to_dir(directory.path()).unwrap();
    assert!(sizes.schema_bytes > 0);
    assert!(sizes.graph_bytes > 0);

    let manifest_before = fs::read(directory.path().join("manifest.json")).unwrap();
    let entries_before = fs::read_dir(directory.path().join(".snapshots"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    GraphIndex::validate_dir(directory.path()).unwrap();
    assert_eq!(
        fs::read(directory.path().join("manifest.json")).unwrap(),
        manifest_before
    );
    assert_eq!(
        fs::read_dir(directory.path().join(".snapshots"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        entries_before
    );

    let restored = GraphIndex::load_from_dir(directory.path()).unwrap();
    assert_eq!(restored.graph_query(&query(), None).unwrap(), expected);
}

#[test]
fn a_second_save_atomically_selects_a_complete_new_generation() {
    let directory = TestDirectory::new("resave");
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    graph.save_to_dir(directory.path()).unwrap();
    let first = active_generation(directory.path());
    graph.save_to_dir(directory.path()).unwrap();
    let second = active_generation(directory.path());

    assert_ne!(first, second);
    assert!(!first.exists());
    assert!(second.is_dir());
    GraphIndex::validate_dir(directory.path()).unwrap();
}

#[test]
fn abandoned_staging_directory_is_never_loaded() {
    let directory = TestDirectory::new("abandoned");
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    graph.save_to_dir(directory.path()).unwrap();
    let abandoned = directory.path().join(".snapshots/.staging-abandoned");
    fs::create_dir(&abandoned).unwrap();
    fs::write(abandoned.join("graph.bin"), b"partial").unwrap();

    GraphIndex::validate_dir(directory.path()).unwrap();
    assert!(abandoned.exists());
    graph.save_to_dir(directory.path()).unwrap();
    assert!(!abandoned.exists());
}

#[test]
fn writer_lock_is_process_safe_and_released_by_the_os_handle() {
    let directory = TestDirectory::new("writer-lock");
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    let lock_path = directory.path().join(".writer.lock");
    let lock = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .unwrap();
    lock.lock_exclusive().unwrap();
    assert!(matches!(
        graph.save_to_dir(directory.path()).unwrap_err(),
        GraphError::WriterBusy { .. }
    ));
    drop(lock);
    graph.save_to_dir(directory.path()).unwrap();
}

#[test]
fn loaded_generation_lease_defers_cleanup_until_reader_drop() {
    let directory = TestDirectory::new("leases");
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    graph.save_to_dir(directory.path()).unwrap();
    let first = active_generation(directory.path());
    let reader = GraphIndex::load_from_dir(directory.path()).unwrap();

    graph.save_to_dir(directory.path()).unwrap();
    let second = active_generation(directory.path());
    assert_ne!(first, second);
    assert!(first.exists());

    drop(reader);
    graph.save_to_dir(directory.path()).unwrap();
    let third = active_generation(directory.path());
    assert_ne!(second, third);
    assert!(!first.exists());
    assert!(!second.exists());
    assert!(third.exists());
}

#[test]
fn graph_payload_size_and_checksum_corruption_are_rejected() {
    for (label, mutation) in [("truncated", 0_u8), ("appended", 1_u8), ("same-size", 2_u8)] {
        let directory = TestDirectory::new(label);
        let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
        graph.save_to_dir(directory.path()).unwrap();
        let graph_path = active_generation(directory.path()).join("graph.bin");
        let mut bytes = fs::read(&graph_path).unwrap();
        match mutation {
            0 => {
                bytes.pop();
            }
            1 => bytes.push(0),
            _ => {
                let middle = bytes.len() / 2;
                bytes[middle] ^= 1;
            }
        }
        fs::write(graph_path, bytes).unwrap();
        assert!(matches!(
            GraphIndex::load_from_dir(directory.path()).unwrap_err(),
            GraphError::InvalidSnapshot { .. }
        ));
    }
}

#[test]
fn manifest_rejects_unsafe_snapshot_paths_and_schema_corruption() {
    let directory = TestDirectory::new("manifest");
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    graph.save_to_dir(directory.path()).unwrap();
    let schema_path = active_generation(directory.path()).join("schema.json");
    let mut schema = fs::read(&schema_path).unwrap();
    schema[0] ^= 1;
    fs::write(schema_path, schema).unwrap();
    assert!(matches!(
        GraphIndex::validate_dir(directory.path()).unwrap_err(),
        GraphError::InvalidSnapshot { .. }
    ));

    graph.save_to_dir(directory.path()).unwrap();
    let manifest_path = directory.path().join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["snapshot_id"] = Value::String("../escape".to_owned());
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(matches!(
        GraphIndex::validate_dir(directory.path()).unwrap_err(),
        GraphError::InvalidSnapshot { .. }
    ));
}

#[test]
fn save_does_not_clean_generations_when_existing_manifest_is_invalid() {
    let directory = TestDirectory::new("invalid-manifest-save");
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    graph.save_to_dir(directory.path()).unwrap();
    let active = active_generation(directory.path());
    fs::write(directory.path().join("manifest.json"), b"not-json").unwrap();

    assert!(matches!(
        graph.save_to_dir(directory.path()).unwrap_err(),
        GraphError::InvalidSnapshot { .. }
    ));
    assert!(active.exists());
}
