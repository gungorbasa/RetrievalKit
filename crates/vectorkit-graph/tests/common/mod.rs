#![allow(dead_code)]

use std::collections::BTreeMap;

use vectorkit_core::{
    ChunkKey, CorpusId, ExactVectorIndex, FieldName, IndexConfig, Metadata, MetadataValue, Record,
    RecordChunkInput, RecordId, RecordType, RecordValue, VectorEncoding, VectorMetric,
};
use vectorkit_graph::{
    Cardinality, ChunkNodeSchema, DuplicateReferencePolicy, FieldPath, GraphSchema,
    MissingTargetPolicy, NodeId, NodeType, RecordNodeSchema, RelationshipSchema, RelationshipType,
};

pub fn field(name: &str) -> FieldName {
    FieldName::new(name).unwrap()
}

pub fn node_type(name: &str) -> NodeType {
    NodeType::new(name).unwrap()
}

pub fn relationship(name: &str) -> RelationshipType {
    RelationshipType::new(name).unwrap()
}

pub fn record_node(node_type_name: &str, record_id: &str) -> NodeId {
    NodeId::record(node_type(node_type_name), RecordId::new(record_id).unwrap())
}

pub fn social_schema() -> GraphSchema {
    GraphSchema::new(vec![
        RecordNodeSchema {
            record_type: RecordType::new("Person").unwrap(),
            node_type: node_type("Person"),
            queryable_fields: vec![
                FieldPath::single(field("name")),
                FieldPath::single(field("active")),
            ],
        },
        RecordNodeSchema {
            record_type: RecordType::new("Project").unwrap(),
            node_type: node_type("Project"),
            queryable_fields: vec![FieldPath::single(field("name"))],
        },
    ])
    .with_relationships(vec![
        RelationshipSchema {
            relationship_type: relationship("WORKS_ON"),
            source_node_type: node_type("Person"),
            target_node_type: node_type("Project"),
            source_field: FieldPath::single(field("project_ids")),
            cardinality: Cardinality::Many,
            missing_target: MissingTargetPolicy::Error,
            duplicate_references: DuplicateReferencePolicy::Error,
            allow_self_edge: false,
            inverse_relationship: Some(relationship("HAS_MEMBER")),
        },
        RelationshipSchema {
            relationship_type: relationship("KNOWS"),
            source_node_type: node_type("Person"),
            target_node_type: node_type("Person"),
            source_field: FieldPath::single(field("knows")),
            cardinality: Cardinality::Many,
            missing_target: MissingTargetPolicy::Error,
            duplicate_references: DuplicateReferencePolicy::Error,
            allow_self_edge: false,
            inverse_relationship: None,
        },
    ])
    .with_chunk_nodes(ChunkNodeSchema {
        node_type: node_type("Chunk"),
        owns_relationship: relationship("HAS_CHUNK"),
        inverse_relationship: Some(relationship("CHUNK_OF")),
    })
}

pub fn social_core(reverse: bool) -> ExactVectorIndex {
    let config =
        IndexConfig::new(3, VectorMetric::DotProduct).with_vector_encoding(VectorEncoding::F32);
    let mut index = ExactVectorIndex::try_with_config_in_corpus(
        config,
        CorpusId::new("generic-social").unwrap(),
    )
    .unwrap();
    let mut records = vec![
        social_record(
            "alice",
            "Person",
            "Alice",
            &["project-a", "project-b"],
            &["bob"],
            true,
            vec![1.0, 0.0, 0.0],
        ),
        social_record(
            "bob",
            "Person",
            "Bob",
            &["project-a"],
            &["carol"],
            true,
            vec![0.8, 0.1, 0.0],
        ),
        social_record(
            "carol",
            "Person",
            "Carol",
            &[],
            &["alice"],
            false,
            vec![0.6, 0.2, 0.0],
        ),
        social_record(
            "project-a",
            "Project",
            "Analytical Engine",
            &[],
            &[],
            true,
            vec![0.0, 1.0, 0.0],
        ),
        social_record(
            "project-b",
            "Project",
            "Difference Engine",
            &[],
            &[],
            true,
            vec![0.0, 0.8, 0.1],
        ),
    ];
    if reverse {
        records.reverse();
    }
    for (record, metadata, chunk) in records {
        index.upsert_record(record, metadata, vec![chunk]).unwrap();
    }
    index
}

fn social_record(
    id: &str,
    record_type: &str,
    name: &str,
    projects: &[&str],
    knows: &[&str],
    active: bool,
    embedding: Vec<f32>,
) -> (Record, Metadata, RecordChunkInput) {
    let mut fields = BTreeMap::from([
        (field("name"), RecordValue::String(name.to_owned())),
        (field("active"), RecordValue::Bool(active)),
    ]);
    if record_type == "Person" {
        fields.insert(
            field("project_ids"),
            RecordValue::List(
                projects
                    .iter()
                    .map(|id| RecordValue::String((*id).to_owned()))
                    .collect(),
            ),
        );
        fields.insert(
            field("knows"),
            RecordValue::List(
                knows
                    .iter()
                    .map(|id| RecordValue::String((*id).to_owned()))
                    .collect(),
            ),
        );
    }
    let record = Record {
        id: RecordId::new(id).unwrap(),
        record_type: RecordType::new(record_type).unwrap(),
        fields,
        content: Some(format!("{name} canonical record")),
    };
    let metadata = BTreeMap::from([(
        "kind".to_owned(),
        MetadataValue::String(record_type.to_ascii_lowercase()),
    )]);
    let chunk = RecordChunkInput {
        key: ChunkKey::new("body").unwrap(),
        text: format!("{name} searchable content"),
        embedding,
        metadata: Metadata::new(),
    };
    (record, metadata, chunk)
}
