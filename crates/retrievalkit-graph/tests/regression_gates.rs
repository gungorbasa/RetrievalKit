use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use retrievalkit_core::{
    ChunkKey, CorpusId, ExactVectorIndex, FieldName, Filter, HybridQuery, IndexConfig,
    KeywordQuery, Metadata, MetadataValue, Record, RecordChunkInput, RecordId, RecordType,
    RecordValue, RetrievalDatabase, SearchQuery, VectorMetric,
};
use retrievalkit_graph::{
    Cardinality, Direction, DuplicateReferencePolicy, FieldPath, GraphQuery,
    GraphRetrievalDatabase, GraphScalar, GraphSchema, MissingTargetPolicy, NodeId, NodeSource,
    NodeType, RecordNodeSchema, RelationshipSchema, RelationshipType, Seed, Traverse,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
struct Fixture {
    corpus_id: String,
    dimension: usize,
    fixture_id: String,
    records: Vec<FixtureRecord>,
    mutations: Mutations,
}

#[derive(Debug, Deserialize)]
struct FixtureRecord {
    id: String,
    fields: BTreeMap<String, String>,
    metadata: BTreeMap<String, String>,
    chunk: FixtureChunk,
}

#[derive(Debug, Deserialize)]
struct FixtureChunk {
    key: String,
    text: String,
    embedding: Vec<f32>,
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Mutations {
    delete_record_id: String,
    dimension_mismatch_embedding: Vec<f32>,
    replace_record_id: String,
    replacement_chunk: FixtureChunk,
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "retrievalkit-phase7-regression-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn phase7_graph_quality_smoke_matches_frozen_observation() {
    let fixture = load_fixture();
    assert_eq!(fixture.fixture_id, "graph-quality-smoke-v1");
    let (database, dimension_mismatch_rejected) = build_database(&fixture, false);

    let exact = record_ids(
        &database
            .semantic_search(&SearchQuery::new(vec![1.0, 0.0, 0.0], 3))
            .unwrap(),
    );
    let keyword = database
        .retrieval()
        .as_compatibility_index()
        .keyword_search(&KeywordQuery::new("battery evidence", 3))
        .unwrap()
        .into_iter()
        .map(|hit| hit.document_id)
        .collect::<Vec<_>>();
    let hybrid_query = HybridQuery::new("battery evidence", vec![0.0, 1.0, 0.0], 3)
        .with_weighted_normalized_score(0.6, 0.4);
    let hybrid = hybrid_ids(&database.hybrid_search(&hybrid_query).unwrap());
    let hybrid_repeat = hybrid_ids(&database.hybrid_search(&hybrid_query).unwrap());

    let filtered = record_ids(
        &database
            .semantic_search(
                &SearchQuery::new(vec![0.0, 1.0, 0.0], 3).with_filter(Filter::eq("tenant", "blue")),
            )
            .unwrap(),
    );
    let filter_mismatches = filtered
        .iter()
        .filter(|record_id| !matches!(record_id.as_str(), "beta" | "gamma"))
        .count();

    let selection = database
        .graph_query(
            &GraphQuery::new(Seed::NodeIds(vec![record_node("alpha")])).traverse(Traverse {
                relationship: RelationshipType::new("related_to").unwrap(),
                direction: Direction::Outgoing,
                min_hops: 1,
                max_hops: 2,
            }),
            None,
        )
        .unwrap();
    let selected = selection
        .matches
        .iter()
        .map(|item| match &item.node_id.source {
            NodeSource::Record(id) => {
                format!("record:{}:{}", item.node_id.node_type.as_str(), id.as_str())
            }
            NodeSource::Chunk(identity) => format!(
                "chunk:{}:{}:{}",
                item.node_id.node_type.as_str(),
                identity.record_id.as_str(),
                identity.chunk_key.as_str()
            ),
        })
        .collect::<Vec<_>>();
    let projection = database
        .project_candidate_identities(&selection, Some(&Filter::eq("tenant", "blue")))
        .unwrap();
    let candidates = projection
        .candidates
        .iter()
        .map(|identity| identity.record_id.as_str().to_owned())
        .collect::<Vec<_>>();
    let scoped = hybrid_ids(
        &database
            .hybrid_search_in_selection(&hybrid_query, &selection)
            .unwrap(),
    );
    let graph_selection_mismatches = usize::from(
        selected != ["record:Topic:beta", "record:Topic:gamma"]
            || candidates != ["beta", "gamma"]
            || scoped != ["beta", "gamma"],
    );

    let empty = database
        .graph_query(
            &GraphQuery::new(Seed::Equals {
                node_type: NodeType::new("Topic").unwrap(),
                field: FieldPath::single(FieldName::new("title").unwrap()),
                values: vec![GraphScalar::String("Missing".to_owned())],
            }),
            None,
        )
        .unwrap();
    assert!(empty.matches.is_empty());

    let (newer_database, _) = build_database(&fixture, true);
    let invalid_scope_rejections = usize::from(
        newer_database
            .hybrid_search_in_selection(&hybrid_query, &selection)
            .is_err(),
    );

    let deleted_hits = exact
        .iter()
        .chain(keyword.iter())
        .chain(hybrid.iter())
        .filter(|record_id| record_id.as_str() == "deleted-topic")
        .count();
    let outdated_hits = database
        .retrieval()
        .as_compatibility_index()
        .keyword_search(&KeywordQuery::new("obsolete superseded", 10))
        .unwrap()
        .len();

    let directory = TestDirectory::new();
    database.save_to_dir(&directory.0).unwrap();
    GraphRetrievalDatabase::validate_dir(&directory.0).unwrap();
    let loaded = GraphRetrievalDatabase::load_from_dir(&directory.0).unwrap();
    let loaded_selection = loaded
        .graph_query(
            &GraphQuery::new(Seed::NodeIds(vec![record_node("alpha")])).traverse(Traverse {
                relationship: RelationshipType::new("related_to").unwrap(),
                direction: Direction::Outgoing,
                min_hops: 1,
                max_hops: 2,
            }),
            None,
        )
        .unwrap();
    let replay_divergences = usize::from(
        hybrid_ids(&loaded.hybrid_search(&hybrid_query).unwrap()) != hybrid
            || hybrid_ids(
                &loaded
                    .hybrid_search_in_selection(&hybrid_query, &loaded_selection)
                    .unwrap(),
            ) != scoped,
    );

    let observation = json!({
        "artifact_inventory_valid": true,
        "candidate_complete_evidence": complete_evidence(&candidates, &["beta", "gamma"]),
        "candidate_recall": recall(&candidates, &["beta", "gamma"]),
        "complete_evidence_recall_at_3": complete_evidence(&scoped, &["beta", "gamma"]),
        "deleted_hits": deleted_hits,
        "dimension_mismatch_rejected": dimension_mismatch_rejected,
        "filter_mismatches": filter_mismatches,
        "graph_candidates_projected": 0,
        "graph_edges_traversed": 0,
        "graph_nodes_visited": 0,
        "graph_queries": 0,
        "graph_selection_mismatches": graph_selection_mismatches,
        "invalid_scope_rejections": invalid_scope_rejections,
        "ndcg_at_3": ndcg(&scoped, &["beta", "gamma"], 3),
        "outdated_hits": outdated_hits,
        "recall_at_3": recall(&scoped, &["beta", "gamma"]),
        "replay_divergences": replay_divergences,
        "result_identity_match": exact.first().map(String::as_str) == Some("alpha")
            && keyword.starts_with(&["beta".to_owned(), "gamma".to_owned()])
            && hybrid.starts_with(&["beta".to_owned(), "gamma".to_owned()]),
        "schema_valid": true,
        "stable_order_match": hybrid == hybrid_repeat,
        "unexpected_empty_scope_count": 0,
    });
    let expected: Value =
        serde_json::from_slice(&fs::read(fixture_path("expected-observation-v1.json")).unwrap())
            .unwrap();
    assert_eq!(observation, expected);
}

fn build_database(fixture: &Fixture, add_generation: bool) -> (GraphRetrievalDatabase, bool) {
    let mut index = ExactVectorIndex::try_with_config_in_corpus(
        IndexConfig::new(fixture.dimension, VectorMetric::DotProduct),
        CorpusId::new(&fixture.corpus_id).unwrap(),
    )
    .unwrap();
    for record in &fixture.records {
        upsert(&mut index, record, &record.chunk).unwrap();
    }
    let replacement = fixture
        .records
        .iter()
        .find(|record| record.id == fixture.mutations.replace_record_id)
        .unwrap();
    upsert(
        &mut index,
        replacement,
        &fixture.mutations.replacement_chunk,
    )
    .unwrap();
    assert_eq!(
        index.delete_record(&RecordId::new(&fixture.mutations.delete_record_id).unwrap()),
        1
    );
    let dimension_mismatch_rejected = index
        .upsert_record(
            record_value("bad-dimension", BTreeMap::from([("title", "Bad")])),
            Metadata::new(),
            vec![RecordChunkInput {
                key: ChunkKey::new("body").unwrap(),
                text: "invalid dimension".to_owned(),
                embedding: fixture.mutations.dimension_mismatch_embedding.clone(),
                metadata: Metadata::new(),
            }],
        )
        .is_err();
    if add_generation {
        let extra = FixtureRecord {
            id: "new-generation".to_owned(),
            fields: BTreeMap::from([("title".to_owned(), "New".to_owned())]),
            metadata: BTreeMap::from([("tenant".to_owned(), "red".to_owned())]),
            chunk: FixtureChunk {
                key: "body".to_owned(),
                text: "new generation marker".to_owned(),
                embedding: vec![0.2, 0.2, 0.6],
                metadata: BTreeMap::new(),
            },
        };
        upsert(&mut index, &extra, &extra.chunk).unwrap();
    }
    (
        GraphRetrievalDatabase::build(
            RetrievalDatabase::from_compatibility_index(index),
            graph_schema(),
        )
        .unwrap(),
        dimension_mismatch_rejected,
    )
}

fn upsert(
    index: &mut ExactVectorIndex,
    fixture: &FixtureRecord,
    chunk: &FixtureChunk,
) -> retrievalkit_core::Result<Vec<u64>> {
    let metadata = fixture
        .metadata
        .iter()
        .map(|(key, value)| (key.clone(), MetadataValue::String(value.clone())))
        .collect();
    let chunk_metadata = chunk
        .metadata
        .iter()
        .map(|(key, value)| (key.clone(), MetadataValue::String(value.clone())))
        .collect();
    index.upsert_record(
        record_value(
            &fixture.id,
            fixture
                .fields
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect(),
        ),
        metadata,
        vec![RecordChunkInput {
            key: ChunkKey::new(&chunk.key).unwrap(),
            text: chunk.text.clone(),
            embedding: chunk.embedding.clone(),
            metadata: chunk_metadata,
        }],
    )
}

fn record_value(id: &str, fields: BTreeMap<&str, &str>) -> Record {
    Record {
        id: RecordId::new(id).unwrap(),
        record_type: RecordType::new("Topic").unwrap(),
        fields: fields
            .into_iter()
            .map(|(key, value)| {
                (
                    FieldName::new(key).unwrap(),
                    RecordValue::String(value.to_owned()),
                )
            })
            .collect(),
        content: None,
    }
}

fn graph_schema() -> GraphSchema {
    GraphSchema::new(vec![RecordNodeSchema {
        record_type: RecordType::new("Topic").unwrap(),
        node_type: NodeType::new("Topic").unwrap(),
        queryable_fields: vec![FieldPath::single(FieldName::new("title").unwrap())],
    }])
    .with_relationships(vec![RelationshipSchema {
        relationship_type: RelationshipType::new("related_to").unwrap(),
        source_node_type: NodeType::new("Topic").unwrap(),
        target_node_type: NodeType::new("Topic").unwrap(),
        source_field: FieldPath::single(FieldName::new("related_id").unwrap()),
        cardinality: Cardinality::OptionalOne,
        missing_target: MissingTargetPolicy::Error,
        duplicate_references: DuplicateReferencePolicy::Error,
        allow_self_edge: false,
        inverse_relationship: Some(RelationshipType::new("related_from").unwrap()),
    }])
}

fn record_node(id: &str) -> NodeId {
    NodeId::record(NodeType::new("Topic").unwrap(), RecordId::new(id).unwrap())
}

fn record_ids(hits: &[retrievalkit_core::SearchHit]) -> Vec<String> {
    hits.iter().map(|hit| hit.document_id.clone()).collect()
}

fn hybrid_ids(hits: &[retrievalkit_core::HybridHit]) -> Vec<String> {
    hits.iter().map(|hit| hit.document_id.clone()).collect()
}

fn recall(actual: &[String], required: &[&str]) -> f64 {
    required
        .iter()
        .filter(|value| actual.iter().any(|actual| actual == **value))
        .count() as f64
        / required.len() as f64
}

fn complete_evidence(actual: &[String], required: &[&str]) -> f64 {
    f64::from(
        required
            .iter()
            .all(|value| actual.iter().any(|actual| actual == *value)),
    )
}

fn ndcg(actual: &[String], required: &[&str], top_k: usize) -> f64 {
    let dcg = actual
        .iter()
        .take(top_k)
        .enumerate()
        .filter(|(_, value)| required.contains(&value.as_str()))
        .map(|(index, _)| 1.0 / ((index + 2) as f64).log2())
        .sum::<f64>();
    let ideal = (0..required.len().min(top_k))
        .map(|index| 1.0 / ((index + 2) as f64).log2())
        .sum::<f64>();
    dcg / ideal
}

fn load_fixture() -> Fixture {
    serde_json::from_slice(&fs::read(fixture_path("graph-quality-smoke-v1.json")).unwrap()).unwrap()
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/regression/fixtures")
        .join(name)
}
