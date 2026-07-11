# vectorkit-graph

Optional, fully local, schema-driven graph retrieval for VectorKit.

The package does not prescribe people/projects, products/categories, notes,
files, or any other domain. Applications map their own canonical `RecordType`s
to graph `NodeType`s and declare typed relationships over record field paths.

```rust
use vectorkit_core::{FieldName, RecordType};
use vectorkit_graph::{
    Cardinality, DuplicateReferencePolicy, FieldPath, GraphIndex, GraphSchema,
    MissingTargetPolicy, NodeType, RecordNodeSchema, RelationshipSchema,
    RelationshipType,
};

let product = NodeType::new("Product")?;
let category = NodeType::new("Category")?;
let schema = GraphSchema::new(vec![
    RecordNodeSchema {
        record_type: RecordType::new("Product")?,
        node_type: product.clone(),
        queryable_fields: vec![FieldPath::single(FieldName::new("sku")?)],
    },
    RecordNodeSchema {
        record_type: RecordType::new("Category")?,
        node_type: category.clone(),
        queryable_fields: vec![FieldPath::single(FieldName::new("name")?)],
    },
])
.with_relationships(vec![RelationshipSchema {
    relationship_type: RelationshipType::new("IN_CATEGORY")?,
    source_node_type: product,
    target_node_type: category,
    source_field: FieldPath::single(FieldName::new("category_ids")?),
    cardinality: Cardinality::Many,
    missing_target: MissingTargetPolicy::Error,
    duplicate_references: DuplicateReferencePolicy::Error,
    allow_self_edge: false,
    inverse_relationship: Some(RelationshipType::new("HAS_PRODUCT")?),
}]);

// `core` already owns canonical records, chunks, vectors, BM25, and metadata.
let graph = GraphIndex::build(core, schema)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

M2 supports:

- one record-node mapping per canonical record type;
- stable structured record and optional chunk nodes;
- explicit single, optional, and collection references;
- missing-target, duplicate-reference, self-edge, and inverse-edge policies;
- exact property seeds for declared String, I64, and Bool fields;
- deterministic bounded outgoing/incoming multi-step traversal;
- cycles, canonical shortest paths, limits, cancellation, and provenance;
- record/chunk projection into generation-bound `CandidateScope` values;
- delegated exact, BM25, and hybrid ranking without changing core hot paths.

Graph persistence, composite commits/recovery, wrapper APIs, arbitrary query
languages, automatic extraction, analytics, and incremental mutation belong to
later milestones.
