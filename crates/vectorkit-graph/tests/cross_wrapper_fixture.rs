use std::path::PathBuf;

use serde::Deserialize;
use vectorkit_core::{
    ChunkKey, CorpusId, ExactVectorIndex, Filter, IndexConfig, KeywordQuery, Metadata,
    MetadataValue, Record, RecordChunkInput, SearchQuery, VectorMetric,
};
use vectorkit_graph::{
    Direction, FieldPath, GraphIndex, GraphQuery, GraphScalar, GraphSchema, NodeId, NodeSource,
    NodeType, RelationshipType, Seed, Traverse,
};

#[derive(Deserialize)]
struct Fixture {
    schema_version: u32,
    fixture_id: String,
    dimension: usize,
    corpus_id: String,
    records: Vec<RecordBatch>,
    schema: GraphSchema,
    expectations: Expectations,
}

#[derive(Deserialize)]
struct RecordBatch {
    record: Record,
    projected_metadata: Metadata,
    chunks: Vec<Chunk>,
}

#[derive(Deserialize)]
struct Chunk {
    key: ChunkKey,
    text: String,
    embedding: Vec<f32>,
    metadata: Metadata,
}

#[derive(Deserialize)]
struct Expectations {
    equality: EqualityExpectation,
    traversal: TraversalExpectation,
    filtered_exact: FilteredExactExpectation,
    keyword: KeywordExpectation,
}

#[derive(Deserialize)]
struct EqualityExpectation {
    node_type: String,
    field: FieldPath,
    value: String,
    node_ids: Vec<String>,
    source_nodes: usize,
    resolved_chunks: usize,
}

#[derive(Deserialize)]
struct TraversalExpectation {
    seed_record_id: String,
    relationship: String,
    min_hops: usize,
    max_hops: usize,
    node_ids: Vec<String>,
    paths: Vec<Vec<String>>,
    source_nodes: usize,
    resolved_chunks: usize,
}

#[derive(Deserialize)]
struct FilteredExactExpectation {
    seed_titles: Vec<String>,
    embedding: Vec<f32>,
    filter_field: String,
    filter_value: String,
    record_ids: Vec<String>,
}

#[derive(Deserialize)]
struct KeywordExpectation {
    text: String,
    record_ids: Vec<String>,
}

#[test]
fn rust_matches_the_generic_cross_wrapper_fixture() {
    let fixture = load_fixture();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.fixture_id, "generic-topics-v1");
    let mut core = ExactVectorIndex::try_with_config_in_corpus(
        IndexConfig::new(fixture.dimension, VectorMetric::DotProduct),
        CorpusId::new(fixture.corpus_id).unwrap(),
    )
    .unwrap();
    for batch in fixture.records {
        let chunks = batch
            .chunks
            .into_iter()
            .map(|chunk| RecordChunkInput {
                key: chunk.key,
                text: chunk.text,
                embedding: chunk.embedding,
                metadata: chunk.metadata,
            })
            .collect();
        core.upsert_record(batch.record, batch.projected_metadata, chunks)
            .unwrap();
    }
    let graph = GraphIndex::build(core, fixture.schema).unwrap();

    let equality = &fixture.expectations.equality;
    let equality_result = graph
        .graph_query(
            &GraphQuery::new(Seed::Equals {
                node_type: NodeType::new(&equality.node_type).unwrap(),
                field: equality.field.clone(),
                values: vec![GraphScalar::String(equality.value.clone())],
            }),
            None,
        )
        .unwrap();
    assert_eq!(record_ids(&equality_result.matches), equality.node_ids);
    let equality_scope = graph.project_candidates(&equality_result).unwrap();
    assert_eq!(equality_scope.trace.source_nodes, equality.source_nodes);
    assert_eq!(
        equality_scope.trace.resolved_chunks,
        equality.resolved_chunks
    );

    let traversal = &fixture.expectations.traversal;
    let traversal_result = graph
        .graph_query(
            &GraphQuery::new(Seed::NodeIds(vec![NodeId::record(
                NodeType::new("Topic").unwrap(),
                vectorkit_core::RecordId::new(&traversal.seed_record_id).unwrap(),
            )]))
            .traverse(Traverse {
                relationship: RelationshipType::new(&traversal.relationship).unwrap(),
                direction: Direction::Outgoing,
                min_hops: traversal.min_hops,
                max_hops: traversal.max_hops,
            }),
            None,
        )
        .unwrap();
    assert_eq!(record_ids(&traversal_result.matches), traversal.node_ids);
    assert_eq!(
        traversal_result
            .matches
            .iter()
            .map(|matched| {
                matched
                    .path
                    .iter()
                    .map(|edge| edge.edge_id.relationship_type.as_str().to_owned())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        traversal.paths
    );
    let traversal_scope = graph.project_candidates(&traversal_result).unwrap();
    assert_eq!(traversal_scope.trace.source_nodes, traversal.source_nodes);
    assert_eq!(
        traversal_scope.trace.resolved_chunks,
        traversal.resolved_chunks
    );

    let filtered = &fixture.expectations.filtered_exact;
    let all_result = graph
        .graph_query(
            &GraphQuery::new(Seed::Equals {
                node_type: NodeType::new("Topic").unwrap(),
                field: FieldPath::new(vec![vectorkit_core::FieldName::new("title").unwrap()])
                    .unwrap(),
                values: filtered
                    .seed_titles
                    .iter()
                    .cloned()
                    .map(GraphScalar::String)
                    .collect(),
            }),
            None,
        )
        .unwrap();
    let all_scope = graph.project_candidates(&all_result).unwrap();
    let exact = graph
        .search_in_candidates(
            &SearchQuery::new(filtered.embedding.clone(), 10).with_filter(Filter::eq(
                &filtered.filter_field,
                MetadataValue::String(filtered.filter_value.clone()),
            )),
            &all_scope.scope,
        )
        .unwrap();
    assert_eq!(
        exact
            .iter()
            .map(|hit| hit.document_id.clone())
            .collect::<Vec<_>>(),
        filtered.record_ids
    );
    let keyword = graph
        .keyword_search_in_candidates(
            &KeywordQuery::new(&fixture.expectations.keyword.text, 10),
            &all_scope.scope,
        )
        .unwrap();
    assert_eq!(
        keyword
            .iter()
            .map(|hit| hit.document_id.clone())
            .collect::<Vec<_>>(),
        fixture.expectations.keyword.record_ids
    );
}

fn record_ids(matches: &[vectorkit_graph::GraphMatch]) -> Vec<String> {
    matches
        .iter()
        .map(|matched| match &matched.node_id.source {
            NodeSource::Record(id) => id.as_str().to_owned(),
            NodeSource::Chunk(identity) => identity.record_id.as_str().to_owned(),
        })
        .collect()
}

fn load_fixture() -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/graph-conformance/v1/fixture.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
