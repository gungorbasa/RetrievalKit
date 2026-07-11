mod common;

use vectorkit_core::RecordId;
use vectorkit_graph::{Direction, GraphError, GraphIndex, GraphQuery, GraphScalar, Seed, Traverse};

use common::{field, node_type, record_node, relationship, social_core, social_schema};

fn representative_query() -> GraphQuery {
    GraphQuery::new(Seed::Equals {
        node_type: node_type("Person"),
        field: vectorkit_graph::FieldPath::single(field("name")),
        values: vec![GraphScalar::String("Alice".to_owned())],
    })
    .traverse(Traverse {
        relationship: relationship("WORKS_ON"),
        direction: Direction::Outgoing,
        min_hops: 1,
        max_hops: 1,
    })
}

#[test]
fn snapshot_round_trip_preserves_graph_results_and_stats() {
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    let expected = graph.graph_query(&representative_query(), None).unwrap();
    let payload = graph.snapshot_payload().unwrap();

    let restored = GraphIndex::from_snapshot_payload(social_core(false), &payload).unwrap();
    assert_eq!(restored.schema(), graph.schema());
    assert_eq!(restored.build_stats(), graph.build_stats());
    assert_eq!(restored.node_count(), graph.node_count());
    assert_eq!(restored.edge_count(), graph.edge_count());
    assert_eq!(
        restored.graph_query(&representative_query(), None).unwrap(),
        expected
    );
}

#[test]
fn snapshot_encoding_is_byte_deterministic() {
    let left = GraphIndex::build(social_core(false), social_schema())
        .unwrap()
        .snapshot_payload()
        .unwrap();
    let right = GraphIndex::build(social_core(false), social_schema())
        .unwrap()
        .snapshot_payload()
        .unwrap();
    assert_eq!(left, right);
}

#[test]
fn snapshot_rejects_corruption_truncation_and_trailing_bytes() {
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    let payload = graph.snapshot_payload().unwrap();

    for length in [
        0,
        1,
        4,
        8,
        payload.graph_bytes.len() / 2,
        payload.graph_bytes.len() - 1,
    ] {
        let mut truncated = payload.clone();
        truncated.graph_bytes.truncate(length);
        assert!(matches!(
            GraphIndex::from_snapshot_payload(social_core(false), &truncated).unwrap_err(),
            GraphError::InvalidSnapshot { .. }
        ));
    }

    let mut bad_magic = payload.clone();
    bad_magic.graph_bytes[0] ^= 0xff;
    assert!(matches!(
        GraphIndex::from_snapshot_payload(social_core(false), &bad_magic).unwrap_err(),
        GraphError::InvalidSnapshot { .. }
    ));

    let mut trailing = payload;
    trailing.graph_bytes.push(0);
    assert!(matches!(
        GraphIndex::from_snapshot_payload(social_core(false), &trailing).unwrap_err(),
        GraphError::InvalidSnapshot { .. }
    ));
}

#[test]
fn snapshot_rejects_schema_hash_and_generation_mismatches() {
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    let payload = graph.snapshot_payload().unwrap();

    let mut wrong_hash = payload.clone();
    wrong_hash.schema_hash = vectorkit_graph::SchemaHash::from_bytes([0xff; 32]);
    assert!(matches!(
        GraphIndex::from_snapshot_payload(social_core(false), &wrong_hash).unwrap_err(),
        GraphError::InvalidSnapshot { .. }
    ));

    let mut stale_core = social_core(false);
    assert_eq!(
        stale_core.delete_record(&RecordId::new("alice").unwrap()),
        1
    );
    assert!(matches!(
        GraphIndex::from_snapshot_payload(stale_core, &payload).unwrap_err(),
        GraphError::InvalidSnapshot { .. }
    ));
}

#[test]
fn restored_snapshot_still_projects_candidates() {
    let graph = GraphIndex::build(social_core(false), social_schema()).unwrap();
    let payload = graph.snapshot_payload().unwrap();
    let restored = GraphIndex::from_snapshot_payload(social_core(false), &payload).unwrap();
    let query =
        GraphQuery::new(Seed::NodeIds(vec![record_node("Person", "alice")])).traverse(Traverse {
            relationship: relationship("HAS_CHUNK"),
            direction: Direction::Outgoing,
            min_hops: 1,
            max_hops: 1,
        });
    let result = restored.graph_query(&query, None).unwrap();
    assert_eq!(
        restored
            .project_candidates(&result)
            .unwrap()
            .trace
            .resolved_chunks,
        1
    );
}
