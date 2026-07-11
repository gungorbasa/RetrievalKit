use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use vectorkit_core::{
    ChunkKey, ExactVectorIndex, FieldName, IndexConfig, Metadata, Record, RecordChunkInput,
    RecordId, RecordType, RecordValue, SearchQuery, VectorEncoding, VectorMetric,
};
use vectorkit_graph::{
    Cardinality, Direction, DuplicateReferencePolicy, FieldPath, GraphIndex, GraphQuery,
    GraphSchema, MissingTargetPolicy, NodeId, NodeType, QueryLimits, RecordNodeSchema,
    RelationshipSchema, RelationshipType, Seed, Traverse,
};

const RECORDS: usize = 2_000;
const DEGREE: usize = 4;
const WARMUP: usize = 100;
const SAMPLES: usize = 500;

fn main() {
    let core = build_core();
    let started = Instant::now();
    let graph = GraphIndex::build(core, schema()).unwrap();
    let build = started.elapsed();
    let query = GraphQuery::new(Seed::NodeIds(vec![NodeId::record(
        node_type("Item"),
        RecordId::new("item-0").unwrap(),
    )]))
    .traverse(Traverse {
        relationship: relationship("LINKS"),
        direction: Direction::Outgoing,
        min_hops: 1,
        max_hops: 3,
    })
    .with_limits(QueryLimits {
        max_hops: 3,
        max_visited: 10_000,
        max_results: 10_000,
        max_working_bytes: 16 * 1024 * 1024,
    });
    let result = graph.graph_query(&query, None).unwrap();
    let traversal_p95 = measure(|| graph.graph_query(black_box(&query), None).unwrap());
    let projection_p95 = measure(|| graph.project_candidates(black_box(&result)).unwrap());
    let projected = graph.project_candidates(&result).unwrap();
    let exact_query = SearchQuery::new(vec![1.0; 8], 10);
    let composed_exact_p95 = measure(|| {
        graph
            .search_in_candidates(black_box(&exact_query), black_box(&projected.scope))
            .unwrap()
    });

    println!(
        "{{\"records\":{RECORDS},\"degree\":{DEGREE},\"nodes\":{},\"edges\":{},\"result_nodes\":{},\"build_mode\":\"release\",\"warmup\":{WARMUP},\"samples\":{SAMPLES},\"build_ms\":{},\"traversal_p95_us\":{},\"projection_p95_us\":{},\"composed_exact_p95_ns\":{}}}",
        graph.node_count(),
        graph.edge_count(),
        result.matches.len(),
        build.as_millis(),
        traversal_p95.as_micros(),
        projection_p95.as_micros(),
        composed_exact_p95.as_nanos()
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
    GraphSchema::new(vec![RecordNodeSchema {
        record_type: RecordType::new("Item").unwrap(),
        node_type: node_type("Item"),
        queryable_fields: vec![],
    }])
    .with_relationships(vec![RelationshipSchema {
        relationship_type: relationship("LINKS"),
        source_node_type: node_type("Item"),
        target_node_type: node_type("Item"),
        source_field: FieldPath::single(FieldName::new("links").unwrap()),
        cardinality: Cardinality::Many,
        missing_target: MissingTargetPolicy::Error,
        duplicate_references: DuplicateReferencePolicy::Error,
        allow_self_edge: false,
        inverse_relationship: None,
    }])
}

fn node_type(value: &str) -> NodeType {
    NodeType::new(value).unwrap()
}

fn relationship(value: &str) -> RelationshipType {
    RelationshipType::new(value).unwrap()
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
