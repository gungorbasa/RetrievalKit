use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use retrievalkit_core::{
    ChunkInput, Document, ExactVectorIndex, HybridQuery, IndexConfig, IndexPersistenceOptions,
    KeywordQuery, Metadata, SearchQuery, VectorMetric,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    schema_version: u32,
    fixture_id: String,
    dimension: usize,
    metric: String,
    documents: Vec<FixtureDocument>,
    expectations: Expectations,
}

#[derive(Deserialize)]
struct FixtureDocument {
    id: String,
    metadata: Metadata,
    chunks: Vec<FixtureChunk>,
}

#[derive(Deserialize)]
struct FixtureChunk {
    text: String,
    embedding: Vec<f32>,
    metadata: Metadata,
}

#[derive(Deserialize)]
struct Expectations {
    exact: ExactExpectation,
    keyword: KeywordExpectation,
    hybrid: HybridExpectation,
    alpha_one: IdExpectation,
    alpha_zero: IdExpectation,
    compact_reload_keyword: IdExpectation,
}

#[derive(Deserialize)]
struct ExactExpectation {
    embedding: Vec<f32>,
    document_ids: Vec<String>,
    text: String,
    metadata: Metadata,
}

#[derive(Deserialize)]
struct KeywordExpectation {
    text: String,
    document_ids: Vec<String>,
    matched_terms: Vec<String>,
}

#[derive(Deserialize)]
struct HybridExpectation {
    text: String,
    embedding: Vec<f32>,
    alpha: f32,
    document_ids: Vec<String>,
}

#[derive(Deserialize)]
struct IdExpectation {
    document_ids: Vec<String>,
}

#[test]
fn rust_matches_the_retrieval_cross_wrapper_fixture() {
    let fixture = load_fixture();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.fixture_id, "retrieval-results-v1");
    assert_eq!(fixture.metric, "dot_product");

    let mut index = ExactVectorIndex::try_with_config(IndexConfig::new(
        fixture.dimension,
        VectorMetric::DotProduct,
    ))
    .unwrap();
    for document in fixture.documents {
        index
            .upsert_document(
                Document {
                    id: document.id,
                    text: String::new(),
                    metadata: document.metadata,
                },
                document
                    .chunks
                    .into_iter()
                    .map(|chunk| ChunkInput {
                        text: chunk.text,
                        embedding: chunk.embedding,
                        metadata: chunk.metadata,
                    })
                    .collect(),
            )
            .unwrap();
    }

    let exact = index
        .search(&SearchQuery::new(
            fixture.expectations.exact.embedding.clone(),
            1,
        ))
        .unwrap();
    assert_eq!(
        document_ids(&exact),
        fixture.expectations.exact.document_ids
    );
    let exact_chunk = index.chunk(exact[0].chunk_id).unwrap();
    assert_eq!(exact_chunk.text, fixture.expectations.exact.text);
    assert_eq!(exact_chunk.metadata, fixture.expectations.exact.metadata);

    let keyword = index
        .keyword_search(&KeywordQuery::new(&fixture.expectations.keyword.text, 10))
        .unwrap();
    assert_eq!(
        keyword
            .iter()
            .map(|hit| hit.document_id.clone())
            .collect::<Vec<_>>(),
        fixture.expectations.keyword.document_ids
    );
    assert_eq!(
        keyword[0].matched_terms,
        fixture.expectations.keyword.matched_terms
    );

    let hybrid_expectation = &fixture.expectations.hybrid;
    let hybrid = index
        .hybrid_search(
            &HybridQuery::new(
                &hybrid_expectation.text,
                hybrid_expectation.embedding.clone(),
                10,
            )
            .with_candidate_limits(1, 1)
            .try_with_alpha(hybrid_expectation.alpha)
            .unwrap(),
        )
        .unwrap();
    assert_eq!(document_ids(&hybrid), hybrid_expectation.document_ids);

    let alpha_one = index
        .hybrid_search(
            &HybridQuery::new(
                &hybrid_expectation.text,
                hybrid_expectation.embedding.clone(),
                10,
            )
            .with_candidate_limits(1, 1)
            .try_with_alpha(1.0)
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        document_ids(&alpha_one),
        fixture.expectations.alpha_one.document_ids
    );
    assert!(alpha_one[0].keyword_score.is_none());
    assert!(alpha_one[0].trace.keyword_rank.is_none());

    let alpha_zero = index
        .hybrid_search(
            &HybridQuery::new(&hybrid_expectation.text, Vec::new(), 10)
                .with_candidate_limits(1, 1)
                .try_with_alpha(0.0)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        document_ids(&alpha_zero),
        fixture.expectations.alpha_zero.document_ids
    );
    assert!(alpha_zero[0].vector_score.is_none());
    assert!(alpha_zero[0].trace.vector_rank.is_none());

    let directory = temporary_directory();
    index
        .save_to_dir_with_options(&directory, IndexPersistenceOptions::vector_only())
        .unwrap();
    let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
    let rebuilt_keyword = loaded
        .keyword_search(&KeywordQuery::new(&fixture.expectations.keyword.text, 10))
        .unwrap();
    assert_eq!(
        rebuilt_keyword
            .iter()
            .map(|hit| hit.document_id.clone())
            .collect::<Vec<_>>(),
        fixture.expectations.compact_reload_keyword.document_ids
    );
    fs::remove_dir_all(directory).unwrap();
}

fn document_ids<T>(hits: &[T]) -> Vec<String>
where
    T: HasDocumentId,
{
    hits.iter()
        .map(|hit| hit.document_id().to_owned())
        .collect()
}

trait HasDocumentId {
    fn document_id(&self) -> &str;
}

impl HasDocumentId for retrievalkit_core::SearchHit {
    fn document_id(&self) -> &str {
        &self.document_id
    }
}

impl HasDocumentId for retrievalkit_core::HybridHit {
    fn document_id(&self) -> &str {
        &self.document_id
    }
}

fn load_fixture() -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/retrieval-conformance/v1/fixture.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn temporary_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "retrievalkit-cross-wrapper-{}-{nonce}",
        std::process::id()
    ))
}
