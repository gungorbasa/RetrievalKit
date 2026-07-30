use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;

use retrievalkit_core::{
    CorpusId, HybridHit, HybridQuery, KeywordHit, KeywordQuery, Metadata, Record, RecordId,
    RecordType, SearchHit, SearchQuery, VectorEncoding, VectorMetric,
};
use retrievalkit_graph::{
    GraphQuery, GraphRetrievalDatabase, GraphRetrievalDatabaseBuilder, GraphSchema, NodeId,
    NodeType, RecordNodeSchema, Seed,
};
use serde_json::{json, Value};

const DIMENSION: usize = 384;
const CORPUS_COUNT: usize = 48;
const QUERY_COUNT: usize = 42;
const TOP_K: usize = 10;
const MEAN_OVERLAP_GATE: f64 = 0.99;
const EXACT_SET_GATE: f64 = 0.90;
const MINIMUM_OVERLAP_GATE: f64 = 0.90;

type Embeddings = Vec<Vec<f32>>;
type RankedPairs = Vec<(Vec<String>, Vec<String>)>;

struct DatabaseFixture {
    database: GraphRetrievalDatabase,
    selection: retrievalkit_graph::GraphResult,
}

struct DirectionResults {
    vector: RankedPairs,
    hybrid: RankedPairs,
    scoped_vector: RankedPairs,
    scoped_hybrid: RankedPairs,
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        return Err(
            "usage: qualify_retrieval_policy <texts.json> <onnx-fp32.json> <coreml-fp32.json>"
                .into(),
        );
    }

    let texts: Vec<String> = read_json(&arguments[0])?;
    let onnx: Embeddings = read_json(&arguments[1])?;
    let coreml: Embeddings = read_json(&arguments[2])?;
    validate_fixture(&texts, &onnx, &coreml)?;

    let onnx_f32 = build_database(&texts, &onnx, VectorEncoding::F32)?;
    let onnx_i8 = build_database(&texts, &onnx, VectorEncoding::I8ScalarQuantized)?;
    let coreml_f32 = build_database(&texts, &coreml, VectorEncoding::F32)?;
    let coreml_i8 = build_database(&texts, &coreml, VectorEncoding::I8ScalarQuantized)?;

    assert_eq!(onnx_f32.selection, onnx_i8.selection);
    assert_eq!(onnx_f32.selection, coreml_f32.selection);
    assert_eq!(onnx_f32.selection, coreml_i8.selection);

    let mut keyword_identical = true;
    let mut scoped_keyword_identical = true;
    for query_index in query_range() {
        let query_text = &texts[query_index];
        let keyword_query = KeywordQuery::new(query_text, TOP_K);
        let expected = onnx_f32.database.keyword_search(&keyword_query)?;
        keyword_identical &=
            identical_keyword_hits(&expected, &onnx_i8.database.keyword_search(&keyword_query)?)
                && identical_keyword_hits(
                    &expected,
                    &coreml_f32.database.keyword_search(&keyword_query)?,
                )
                && identical_keyword_hits(
                    &expected,
                    &coreml_i8.database.keyword_search(&keyword_query)?,
                );

        let expected_scoped = onnx_f32
            .database
            .keyword_search_in_selection(&keyword_query, &onnx_f32.selection)?;
        scoped_keyword_identical &= identical_keyword_hits(
            &expected_scoped,
            &onnx_i8
                .database
                .keyword_search_in_selection(&keyword_query, &onnx_i8.selection)?,
        ) && identical_keyword_hits(
            &expected_scoped,
            &coreml_f32
                .database
                .keyword_search_in_selection(&keyword_query, &coreml_f32.selection)?,
        ) && identical_keyword_hits(
            &expected_scoped,
            &coreml_i8
                .database
                .keyword_search_in_selection(&keyword_query, &coreml_i8.selection)?,
        );
    }
    if !keyword_identical || !scoped_keyword_identical {
        return Err("BM25 results changed across provider or vector-storage choices".into());
    }

    let onnx_database_coreml_query =
        compare_direction(&texts, &onnx, &coreml, &onnx_f32, &onnx_i8)?;
    let coreml_database_onnx_query =
        compare_direction(&texts, &coreml, &onnx, &coreml_f32, &coreml_i8)?;

    let first = direction_json(&onnx_database_coreml_query);
    let second = direction_json(&coreml_database_onnx_query);
    if !direction_passed(&first) || !direction_passed(&second) {
        return Err("one or more RetrievalKit Top-10 conformance gates failed".into());
    }

    let report = json!({
        "schema_version": 1,
        "fixture": {
            "corpus_items": CORPUS_COUNT,
            "queries": QUERY_COUNT,
            "dimension": DIMENSION,
            "top_k": TOP_K,
            "graph_selection_items": selection_indices().len(),
        },
        "gates": {
            "mean_top_10_overlap": MEAN_OVERLAP_GATE,
            "exact_top_10_sets": EXACT_SET_GATE,
            "minimum_top_10_overlap": MINIMUM_OVERLAP_GATE,
        },
        "directions": {
            "onnx_built_i8_database_coreml_query": first,
            "coreml_built_i8_database_onnx_query": second,
        },
        "invariants": {
            "bm25_identical": keyword_identical,
            "graph_scoped_bm25_identical": scoped_keyword_identical,
            "graph_only_selection_identical": true,
        },
        "passed": true,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn validate_fixture(
    texts: &[String],
    onnx: &[Vec<f32>],
    coreml: &[Vec<f32>],
) -> Result<(), Box<dyn Error>> {
    let expected = CORPUS_COUNT + QUERY_COUNT + 4;
    if texts.len() != expected || onnx.len() != expected || coreml.len() != expected {
        return Err(format!(
            "expected {expected} texts and vectors, got texts={}, ONNX={}, Core ML={}",
            texts.len(),
            onnx.len(),
            coreml.len()
        )
        .into());
    }
    for (provider, vectors) in [("ONNX", onnx), ("Core ML", coreml)] {
        for (index, vector) in vectors.iter().enumerate() {
            if vector.len() != DIMENSION || vector.iter().any(|value| !value.is_finite()) {
                return Err(format!(
                    "{provider} vector {index} must contain {DIMENSION} finite values"
                )
                .into());
            }
            let norm = vector
                .iter()
                .map(|value| f64::from(*value) * f64::from(*value))
                .sum::<f64>()
                .sqrt();
            if (norm - 1.0).abs() > 1e-3 {
                return Err(format!("{provider} vector {index} has L2 norm {norm}").into());
            }
        }
    }
    Ok(())
}

fn build_database(
    texts: &[String],
    embeddings: &[Vec<f32>],
    encoding: VectorEncoding,
) -> Result<DatabaseFixture, Box<dyn Error>> {
    let record_type = RecordType::new("Document")?;
    let node_type = NodeType::new("Document")?;
    let schema = GraphSchema::new(vec![RecordNodeSchema {
        record_type: record_type.clone(),
        node_type: node_type.clone(),
        queryable_fields: Vec::new(),
    }]);
    let mut builder = GraphRetrievalDatabaseBuilder::new(
        CorpusId::new("fp32-i8-policy")?,
        schema,
        VectorMetric::Cosine,
        encoding,
    );
    for index in 0..CORPUS_COUNT {
        let id = document_id(index);
        builder.upsert_record_with_embedding(
            Record {
                id: RecordId::new(&id)?,
                record_type: record_type.clone(),
                fields: Default::default(),
                content: Some(texts[index].clone()),
            },
            Metadata::new(),
            embeddings[index].clone(),
        )?;
    }
    let database = builder.build()?;
    let selection = database.graph_query(
        &GraphQuery::new(Seed::NodeIds(
            selection_indices()
                .into_iter()
                .map(|index| {
                    Ok(NodeId::record(
                        node_type.clone(),
                        RecordId::new(document_id(index))?,
                    ))
                })
                .collect::<Result<Vec<_>, retrievalkit_core::RetrievalKitError>>()?,
        )),
        None,
    )?;
    Ok(DatabaseFixture {
        database,
        selection,
    })
}

fn compare_direction(
    texts: &[String],
    reference_embeddings: &[Vec<f32>],
    candidate_query_embeddings: &[Vec<f32>],
    reference: &DatabaseFixture,
    candidate: &DatabaseFixture,
) -> Result<DirectionResults, Box<dyn Error>> {
    let mut results = DirectionResults {
        vector: Vec::with_capacity(QUERY_COUNT),
        hybrid: Vec::with_capacity(QUERY_COUNT),
        scoped_vector: Vec::with_capacity(QUERY_COUNT),
        scoped_hybrid: Vec::with_capacity(QUERY_COUNT),
    };
    for query_index in query_range() {
        let text = &texts[query_index];
        let reference_vector = reference_embeddings[query_index].clone();
        let candidate_vector = candidate_query_embeddings[query_index].clone();

        results.vector.push((
            search_ids(
                reference
                    .database
                    .semantic_search(&SearchQuery::new(reference_vector.clone(), TOP_K))?,
            ),
            search_ids(
                candidate
                    .database
                    .semantic_search(&SearchQuery::new(candidate_vector.clone(), TOP_K))?,
            ),
        ));
        results.hybrid.push((
            hybrid_ids(reference.database.hybrid_search(&HybridQuery::new(
                text,
                reference_vector.clone(),
                TOP_K,
            ))?),
            hybrid_ids(candidate.database.hybrid_search(&HybridQuery::new(
                text,
                candidate_vector.clone(),
                TOP_K,
            ))?),
        ));
        results.scoped_vector.push((
            search_ids(reference.database.semantic_search_in_selection(
                &SearchQuery::new(reference_vector.clone(), TOP_K),
                &reference.selection,
            )?),
            search_ids(candidate.database.semantic_search_in_selection(
                &SearchQuery::new(candidate_vector.clone(), TOP_K),
                &candidate.selection,
            )?),
        ));
        results.scoped_hybrid.push((
            hybrid_ids(reference.database.hybrid_search_in_selection(
                &HybridQuery::new(text, reference_vector, TOP_K),
                &reference.selection,
            )?),
            hybrid_ids(candidate.database.hybrid_search_in_selection(
                &HybridQuery::new(text, candidate_vector, TOP_K),
                &candidate.selection,
            )?),
        ));
    }
    Ok(results)
}

fn direction_json(results: &DirectionResults) -> Value {
    json!({
        "vector": overlap_metrics(&results.vector),
        "hybrid": overlap_metrics(&results.hybrid),
        "graph_scoped_vector": overlap_metrics(&results.scoped_vector),
        "graph_scoped_hybrid": overlap_metrics(&results.scoped_hybrid),
    })
}

fn direction_passed(direction: &Value) -> bool {
    [
        "vector",
        "hybrid",
        "graph_scoped_vector",
        "graph_scoped_hybrid",
    ]
    .iter()
    .all(|name| direction[*name]["passed"].as_bool() == Some(true))
}

fn overlap_metrics(pairs: &RankedPairs) -> Value {
    let overlaps = pairs
        .iter()
        .map(|(reference, candidate)| {
            let expected = reference.iter().collect::<BTreeSet<_>>();
            let actual = candidate.iter().collect::<BTreeSet<_>>();
            expected.intersection(&actual).count() as f64 / TOP_K as f64
        })
        .collect::<Vec<_>>();
    let mean = overlaps.iter().sum::<f64>() / overlaps.len() as f64;
    let exact_sets =
        overlaps.iter().filter(|overlap| **overlap == 1.0).count() as f64 / overlaps.len() as f64;
    let exact_order = pairs
        .iter()
        .filter(|(reference, candidate)| reference == candidate)
        .count() as f64
        / pairs.len() as f64;
    let minimum = overlaps.iter().copied().fold(f64::INFINITY, f64::min);
    json!({
        "mean_top_10_overlap": mean,
        "exact_top_10_sets": exact_sets,
        "exact_top_10_order": exact_order,
        "minimum_top_10_overlap": minimum,
        "passed": mean >= MEAN_OVERLAP_GATE
            && exact_sets >= EXACT_SET_GATE
            && minimum >= MINIMUM_OVERLAP_GATE,
    })
}

fn identical_keyword_hits(expected: &[KeywordHit], actual: &[KeywordHit]) -> bool {
    expected == actual
}

fn search_ids(hits: Vec<SearchHit>) -> Vec<String> {
    hits.into_iter().map(|hit| hit.document_id).collect()
}

fn hybrid_ids(hits: Vec<HybridHit>) -> Vec<String> {
    hits.into_iter().map(|hit| hit.document_id).collect()
}

fn query_range() -> std::ops::Range<usize> {
    CORPUS_COUNT..CORPUS_COUNT + QUERY_COUNT
}

fn selection_indices() -> Vec<usize> {
    (0..CORPUS_COUNT).filter(|index| index % 3 != 0).collect()
}

fn document_id(index: usize) -> String {
    format!("document-{index:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_gates_accept_one_boundary_change() {
        let reference = (0..TOP_K).map(document_id).collect::<Vec<_>>();
        let mut one_change = reference.clone();
        one_change[TOP_K - 1] = document_id(TOP_K);
        let pairs = (0..QUERY_COUNT)
            .map(|index| {
                if index == 0 {
                    (reference.clone(), one_change.clone())
                } else {
                    (reference.clone(), reference.clone())
                }
            })
            .collect::<Vec<_>>();

        let metrics = overlap_metrics(&pairs);
        assert_eq!(metrics["minimum_top_10_overlap"], 0.9);
        assert_eq!(metrics["passed"], true);
    }

    #[test]
    fn overlap_gates_reject_two_boundary_changes() {
        let reference = (0..TOP_K).map(document_id).collect::<Vec<_>>();
        let mut two_changes = reference.clone();
        two_changes[TOP_K - 2] = document_id(TOP_K);
        two_changes[TOP_K - 1] = document_id(TOP_K + 1);
        let pairs = (0..QUERY_COUNT)
            .map(|_| (reference.clone(), two_changes.clone()))
            .collect::<Vec<_>>();

        let metrics = overlap_metrics(&pairs);
        assert_eq!(metrics["minimum_top_10_overlap"], 0.8);
        assert_eq!(metrics["passed"], false);
    }
}
