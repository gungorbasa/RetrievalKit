use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vectorkit_core::{
    ChunkKey, ExactVectorIndex, FieldName, IndexConfig, Metadata, Record, RecordChunkInput,
    RecordId, RecordType, RecordValue, VectorEncoding, VectorMetric,
};
use vectorkit_graph::{
    Cardinality, DuplicateReferencePolicy, FieldPath, GraphIndex, GraphSchema, MissingTargetPolicy,
    NodeType, RecordNodeSchema, RelationshipSchema, RelationshipType,
};

const RECORDS: usize = 2_000;
const DEGREE: usize = 4;
const WARMUP: usize = 3;
const SAMPLES: usize = 20;

fn main() {
    let directory = BenchmarkDirectory::new();
    let graph = GraphIndex::build(build_core(), schema()).unwrap();
    let sizes = graph.save_to_dir(&directory.0).unwrap();

    let save_p95 = measure(|| graph.save_to_dir(black_box(&directory.0)).unwrap());
    let open_p95 = measure(|| GraphIndex::load_from_dir(black_box(&directory.0)).unwrap());
    let validate_p95 = measure(|| GraphIndex::validate_dir(black_box(&directory.0)).unwrap());
    let total_bytes = directory_size(&directory.0);

    println!(
        "{{\"records\":{RECORDS},\"degree\":{DEGREE},\"build_mode\":\"release\",\"warmup\":{WARMUP},\"samples\":{SAMPLES},\"percentile\":\"nearest-rank-ceil\",\"save_p95_ms\":{},\"open_p95_ms\":{},\"validate_p95_ms\":{},\"schema_bytes\":{},\"graph_bytes\":{},\"database_bytes\":{total_bytes}}}",
        save_p95.as_millis(),
        open_p95.as_millis(),
        validate_p95.as_millis(),
        sizes.schema_bytes,
        sizes.graph_bytes,
    );
}

fn build_core() -> ExactVectorIndex {
    let config =
        IndexConfig::new(8, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
    let mut core = ExactVectorIndex::try_with_config(config).unwrap();
    for ordinal in 0..RECORDS {
        let links = (1..=DEGREE)
            .map(|distance| RecordValue::String(format!("item-{}", (ordinal + distance) % RECORDS)))
            .collect();
        core.upsert_record(
            Record {
                id: RecordId::new(format!("item-{ordinal}")).unwrap(),
                record_type: RecordType::new("Item").unwrap(),
                fields: BTreeMap::from([(
                    FieldName::new("links").unwrap(),
                    RecordValue::List(links),
                )]),
                content: None,
            },
            Metadata::new(),
            vec![RecordChunkInput {
                key: ChunkKey::new("body").unwrap(),
                text: format!("item {ordinal}"),
                embedding: vec![ordinal as f32 / RECORDS as f32; 8],
                metadata: Metadata::new(),
            }],
        )
        .unwrap();
    }
    core
}

fn schema() -> GraphSchema {
    let item = NodeType::new("Item").unwrap();
    GraphSchema::new(vec![RecordNodeSchema {
        record_type: RecordType::new("Item").unwrap(),
        node_type: item.clone(),
        queryable_fields: vec![],
    }])
    .with_relationships(vec![RelationshipSchema {
        relationship_type: RelationshipType::new("LINKS").unwrap(),
        source_node_type: item.clone(),
        target_node_type: item,
        source_field: FieldPath::single(FieldName::new("links").unwrap()),
        cardinality: Cardinality::Many,
        missing_target: MissingTargetPolicy::Error,
        duplicate_references: DuplicateReferencePolicy::Error,
        allow_self_edge: false,
        inverse_relationship: None,
    }])
}

fn measure<T>(mut operation: impl FnMut() -> T) -> Duration {
    for _ in 0..WARMUP {
        black_box(operation());
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        black_box(operation());
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    samples[(SAMPLES * 95).div_ceil(100) - 1]
}

fn directory_size(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|entry| {
            if entry.is_dir() {
                directory_size(&entry)
            } else {
                entry.metadata().unwrap().len()
            }
        })
        .sum()
}

struct BenchmarkDirectory(PathBuf);

impl BenchmarkDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vectorkit-graph-persistence-bench-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for BenchmarkDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
