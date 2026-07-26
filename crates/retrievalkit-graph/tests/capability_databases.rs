mod common;

use std::collections::BTreeMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use retrievalkit_core::{
    CorpusChunkInput, CorpusId, CorpusIndex, FieldName, Filter, HybridQuery, Metadata, Record,
    RecordId, RecordInput, RecordType, RecordValue, RetrievalDatabase,
};
use retrievalkit_graph::{
    GraphDatabase, GraphQuery, GraphRetrievalDatabase, GraphSchema, NodeId, NodeType,
    RecordNodeSchema, Seed,
};

use common::{social_core, social_schema};

struct TestDirectory(std::path::PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "retrievalkit-{label}-{}-{nonce}",
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
fn graph_only_database_builds_and_persists_without_retrieval_payload() {
    let mut corpus = CorpusIndex::new(CorpusId::new("graph-only").unwrap());
    corpus
        .upsert(RecordInput {
            record: Record {
                id: RecordId::new("rust").unwrap(),
                record_type: RecordType::new("Topic").unwrap(),
                fields: BTreeMap::from([(
                    FieldName::new("title").unwrap(),
                    RecordValue::String("Rust".to_owned()),
                )]),
                content: None,
            },
            metadata: BTreeMap::from([("team".to_owned(), "mobile".into())]),
            chunks: vec![CorpusChunkInput {
                key: retrievalkit_core::ChunkKey::new("summary").unwrap(),
                text: "Rust provides native retrieval".to_owned(),
                metadata: Metadata::new(),
            }],
        })
        .unwrap();
    let schema = GraphSchema::new(vec![RecordNodeSchema {
        record_type: RecordType::new("Topic").unwrap(),
        node_type: NodeType::new("Topic").unwrap(),
        queryable_fields: vec![],
    }]);
    let database = GraphDatabase::build(corpus, schema).unwrap();
    let query = GraphQuery::new(Seed::NodeIds(vec![NodeId::record(
        NodeType::new("Topic").unwrap(),
        RecordId::new("rust").unwrap(),
    )]));
    let selection = database.graph_query(&query, None).unwrap();
    assert_eq!(selection.matches.len(), 1);
    let projected = database
        .project_candidate_identities(&selection, Some(&Filter::eq("team", "mobile")))
        .unwrap();
    assert_eq!(projected.source_nodes, 1);
    assert_eq!(projected.projected_chunks_before_filter, 1);
    assert_eq!(projected.projected_chunks_after_filter, 1);
    assert_eq!(projected.candidates[0].record_id.as_str(), "rust");
    assert_eq!(projected.candidates[0].chunk_key.as_str(), "summary");

    let directory = TestDirectory::new("graph-only");
    let sizes = database.save_to_dir(&directory.0).unwrap();
    assert!(sizes.corpus_bytes > 0);
    assert!(sizes.graph_bytes > 0);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.0.join("manifest.json")).unwrap()).unwrap();
    let snapshot = manifest["snapshot_id"].as_str().unwrap();
    let generation = directory.0.join(".snapshots").join(snapshot);
    assert!(generation.join("corpus/corpus.bin").is_file());
    assert!(generation.join("graph/graph.bin").is_file());
    assert!(!generation.join("retrieval").exists());
    assert!(!generation.join("core").exists());

    let loaded = GraphDatabase::load_from_dir(&directory.0).unwrap();
    assert_eq!(loaded.graph_query(&query, None).unwrap().matches.len(), 1);
    GraphDatabase::validate_dir(&directory.0).unwrap();
}

#[test]
fn graph_retrieval_database_composes_selection_and_round_trips() {
    let retrieval = RetrievalDatabase::from_compatibility_index(social_core(false));
    let database = GraphRetrievalDatabase::build(retrieval, social_schema()).unwrap();
    let selection = database
        .graph_query(
            &GraphQuery::new(Seed::NodeIds(vec![common::record_node("Person", "alice")])),
            None,
        )
        .unwrap();
    let hits = database
        .hybrid_search_in_selection(
            &HybridQuery::new("Alice", vec![1.0, 0.0, 0.0], 3),
            &selection,
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].document_id, "alice");

    let directory = TestDirectory::new("graph-retrieval");
    database.save_to_dir(&directory.0).unwrap();
    let loaded = GraphRetrievalDatabase::load_from_dir(&directory.0).unwrap();
    let loaded_selection = loaded
        .graph_query(
            &GraphQuery::new(Seed::NodeIds(vec![common::record_node("Person", "alice")])),
            None,
        )
        .unwrap();
    assert_eq!(
        loaded
            .hybrid_search_in_selection(
                &HybridQuery::new("Alice", vec![1.0, 0.0, 0.0], 3),
                &loaded_selection,
            )
            .unwrap()[0]
            .document_id,
        "alice"
    );
    GraphRetrievalDatabase::validate_dir(&directory.0).unwrap();
}
