mod common;

use std::collections::BTreeMap;

use vectorkit_core::{
    ChunkKey, ExactVectorIndex, FieldName, IndexConfig, Metadata, Record, RecordChunkInput,
    RecordId, RecordType, RecordValue, VectorEncoding, VectorMetric,
};
use vectorkit_graph::{
    Cardinality, DuplicateReferencePolicy, FieldPath, GraphError, GraphIndex, GraphSchema,
    MissingTargetPolicy, RecordNodeSchema, RelationshipSchema,
};

use common::{field, node_type, relationship, social_core, social_schema};

#[test]
fn generic_schema_build_is_deterministic_across_ingestion_order() {
    let encoded = serde_json::to_vec(&social_schema()).unwrap();
    let decoded: GraphSchema = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, social_schema());
    let left = GraphIndex::build(social_core(false), social_schema()).unwrap();
    let right = GraphIndex::build(social_core(true), social_schema()).unwrap();
    assert_eq!(left.build_stats(), right.build_stats());
    assert_eq!(left.node_count(), 10);
    assert_eq!(left.edge_count(), 19);
}

#[test]
fn schema_rejects_duplicate_node_and_relationship_types() {
    let duplicate_nodes = GraphSchema::new(vec![
        RecordNodeSchema {
            record_type: RecordType::new("One").unwrap(),
            node_type: node_type("Shared"),
            queryable_fields: vec![],
        },
        RecordNodeSchema {
            record_type: RecordType::new("Two").unwrap(),
            node_type: node_type("Shared"),
            queryable_fields: vec![],
        },
    ]);
    assert!(matches!(
        duplicate_nodes.validate().unwrap_err(),
        GraphError::InvalidSchema { .. }
    ));

    let mut duplicate_relationships = social_schema();
    duplicate_relationships.relationships[1].relationship_type = relationship("WORKS_ON");
    assert!(matches!(
        duplicate_relationships.validate().unwrap_err(),
        GraphError::InvalidSchema { .. }
    ));
}

#[test]
fn schema_rejects_unknown_endpoint_and_empty_field_path() {
    assert!(FieldPath::new(Vec::<FieldName>::new()).is_err());
    let schema = GraphSchema::new(vec![RecordNodeSchema {
        record_type: RecordType::new("Person").unwrap(),
        node_type: node_type("Person"),
        queryable_fields: vec![],
    }])
    .with_relationships(vec![RelationshipSchema {
        relationship_type: relationship("BAD"),
        source_node_type: node_type("Person"),
        target_node_type: node_type("Missing"),
        source_field: FieldPath::single(field("target")),
        cardinality: Cardinality::OptionalOne,
        missing_target: MissingTargetPolicy::Error,
        duplicate_references: DuplicateReferencePolicy::Error,
        allow_self_edge: false,
        inverse_relationship: None,
    }]);
    assert!(matches!(
        schema.validate().unwrap_err(),
        GraphError::InvalidSchema { .. }
    ));
}

#[test]
fn missing_target_and_duplicate_reference_policies_are_enforced() {
    let strict = reference_schema(MissingTargetPolicy::Error, DuplicateReferencePolicy::Error);
    assert!(matches!(
        GraphIndex::build(reference_core(&["missing"], false), strict).unwrap_err(),
        GraphError::MissingTarget { .. }
    ));

    let omit = reference_schema(
        MissingTargetPolicy::OmitEdge,
        DuplicateReferencePolicy::Error,
    );
    let graph = GraphIndex::build(reference_core(&["missing"], false), omit).unwrap();
    assert_eq!(graph.edge_count(), 0);
    assert_eq!(graph.build_stats().diagnostics, 1);

    let duplicate_error =
        reference_schema(MissingTargetPolicy::Error, DuplicateReferencePolicy::Error);
    assert!(matches!(
        GraphIndex::build(
            reference_core(&["project", "project"], true),
            duplicate_error
        )
        .unwrap_err(),
        GraphError::InvalidRecord { .. }
    ));

    let deduplicate = reference_schema(
        MissingTargetPolicy::Error,
        DuplicateReferencePolicy::Deduplicate,
    );
    let graph =
        GraphIndex::build(reference_core(&["project", "project"], true), deduplicate).unwrap();
    assert_eq!(graph.edge_count(), 1);
}

fn reference_schema(
    missing_target: MissingTargetPolicy,
    duplicate_references: DuplicateReferencePolicy,
) -> GraphSchema {
    GraphSchema::new(vec![
        RecordNodeSchema {
            record_type: RecordType::new("Person").unwrap(),
            node_type: node_type("Person"),
            queryable_fields: vec![],
        },
        RecordNodeSchema {
            record_type: RecordType::new("Project").unwrap(),
            node_type: node_type("Project"),
            queryable_fields: vec![],
        },
    ])
    .with_relationships(vec![RelationshipSchema {
        relationship_type: relationship("WORKS_ON"),
        source_node_type: node_type("Person"),
        target_node_type: node_type("Project"),
        source_field: FieldPath::single(field("targets")),
        cardinality: Cardinality::Many,
        missing_target,
        duplicate_references,
        allow_self_edge: false,
        inverse_relationship: None,
    }])
}

fn reference_core(targets: &[&str], include_project: bool) -> ExactVectorIndex {
    let config =
        IndexConfig::new(2, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
    let mut core = ExactVectorIndex::try_with_config(config).unwrap();
    let person = Record {
        id: RecordId::new("person").unwrap(),
        record_type: RecordType::new("Person").unwrap(),
        fields: BTreeMap::from([(
            field("targets"),
            RecordValue::List(
                targets
                    .iter()
                    .map(|target| RecordValue::String((*target).to_owned()))
                    .collect(),
            ),
        )]),
        content: None,
    };
    core.upsert_record(
        person,
        Metadata::new(),
        vec![RecordChunkInput {
            key: ChunkKey::new("body").unwrap(),
            text: "person".to_owned(),
            embedding: vec![1.0, 0.0],
            metadata: Metadata::new(),
        }],
    )
    .unwrap();
    if include_project {
        core.upsert_record(
            Record {
                id: RecordId::new("project").unwrap(),
                record_type: RecordType::new("Project").unwrap(),
                fields: BTreeMap::new(),
                content: None,
            },
            Metadata::new(),
            vec![RecordChunkInput {
                key: ChunkKey::new("body").unwrap(),
                text: "project".to_owned(),
                embedding: vec![0.0, 1.0],
                metadata: Metadata::new(),
            }],
        )
        .unwrap();
    }
    core
}
