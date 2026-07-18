use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vectorkit_core::{
    ChunkInput, Document, ExactVectorIndex, HybridQuery, IndexConfig, KeywordQuery, Metadata,
    SearchQuery, VectorEncoding, VectorMetric,
};

use crate::json_to_c_string;

const DIMENSION: usize = 384;
const ACTIVE_RECORDS: usize = 2_500;
const DELETED_RECORDS: usize = 25;
const CHUNKS_PER_RECORD: usize = 4;
const TOP_K: usize = 10;
const WARMUPS: usize = 100;
const SAMPLES: usize = 1_000;
const TARGET_RECORD: usize = 4;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    encoding: String,
    session_id: String,
    product: String,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<Report>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    artifact_type: &'static str,
    workload_id: &'static str,
    encoding: String,
    session_id: String,
    product: String,
    active_chunks: usize,
    deleted_chunks: usize,
    warmups: usize,
    samples: usize,
    percentile_method: &'static str,
    scenarios: Vec<ScenarioReport>,
    graph_counters: GraphCounters,
}

#[derive(Serialize)]
struct ScenarioReport {
    scenario: &'static str,
    raw_duration_ns: Vec<u64>,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    result_identity_sha256: String,
    expected_top_document_id: String,
    observed_top_document_id: String,
    deleted_hits: usize,
}

#[derive(Serialize)]
struct GraphCounters {
    graph_queries: usize,
    graph_nodes_visited: usize,
    graph_edges_traversed: usize,
    graph_candidates_projected: usize,
}

enum Scenario {
    Semantic,
    Bm25,
    Hybrid,
}

/// Runs the graph-free regression workload through APIs shared by the base and
/// graph aggregate products. The result must be freed with
/// `vectorkit_string_free`.
///
/// # Safety
///
/// `config_json` must be a valid null-terminated UTF-8 string for this call.
#[no_mangle]
pub unsafe extern "C" fn vectorkit_phase4_graph_free_regression_json(
    config_json: *const c_char,
) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| unsafe { run(config_json) }))
        .unwrap_or_else(|_| failure("Phase 4b graph-free regression panicked"));
    json_to_c_string(
        &serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_owned()),
    )
}

unsafe fn run(config_json: *const c_char) -> Response {
    if config_json.is_null() {
        return failure("Phase 4b graph-free config cannot be null");
    }
    let raw = match unsafe { CStr::from_ptr(config_json) }.to_str() {
        Ok(value) => value,
        Err(_) => return failure("Phase 4b graph-free config must be valid UTF-8"),
    };
    match run_inner(raw) {
        Ok(report) => Response {
            ok: true,
            report: Some(report),
            error: None,
        },
        Err(error) => failure(&error),
    }
}

fn run_inner(raw: &str) -> Result<Report, String> {
    let config: Config = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if config.session_id.trim().is_empty() {
        return Err("session_id cannot be empty".to_owned());
    }
    if !matches!(config.product.as_str(), "baseline" | "candidate") {
        return Err("product must be baseline or candidate".to_owned());
    }
    let encoding = match config.encoding.as_str() {
        "f32" => VectorEncoding::F32,
        "i8" => VectorEncoding::I8ScalarQuantized,
        _ => return Err("encoding must be f32 or i8".to_owned()),
    };
    let mut index = ExactVectorIndex::try_with_config(
        IndexConfig::new(DIMENSION, VectorMetric::Cosine).with_vector_encoding(encoding),
    )
    .map_err(|error| error.to_string())?;
    populate(&mut index)?;

    let scenarios = [Scenario::Semantic, Scenario::Bm25, Scenario::Hybrid]
        .into_iter()
        .map(|scenario| measure(&index, scenario))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Report {
        schema_version: 1,
        artifact_type: "phase4b_graph_free_regression_session",
        workload_id: "10k-384d-v3",
        encoding: config.encoding,
        session_id: config.session_id,
        product: config.product,
        active_chunks: ACTIVE_RECORDS * CHUNKS_PER_RECORD,
        deleted_chunks: DELETED_RECORDS * CHUNKS_PER_RECORD,
        warmups: WARMUPS,
        samples: SAMPLES,
        percentile_method: "nearest_rank",
        scenarios,
        graph_counters: GraphCounters {
            graph_queries: 0,
            graph_nodes_visited: 0,
            graph_edges_traversed: 0,
            graph_candidates_projected: 0,
        },
    })
}

fn populate(index: &mut ExactVectorIndex) -> Result<(), String> {
    for record_index in 0..ACTIVE_RECORDS {
        upsert(index, record_index, false)?;
    }
    for record_index in 0..DELETED_RECORDS {
        let document_id = record_id(record_index, true);
        upsert(index, record_index, true)?;
        if index.delete_document(&document_id) != CHUNKS_PER_RECORD {
            return Err(format!("failed to delete all chunks for {document_id}"));
        }
    }
    Ok(())
}

fn upsert(index: &mut ExactVectorIndex, record_index: usize, deleted: bool) -> Result<(), String> {
    let document_id = record_id(record_index, deleted);
    let chunks = (0..CHUNKS_PER_RECORD)
        .map(|chunk_index| ChunkInput {
            text: chunk_text(record_index, chunk_index, deleted),
            embedding: source_embedding(record_index, chunk_index, deleted),
            metadata: Metadata::new(),
        })
        .collect();
    index
        .upsert_document(
            Document {
                id: document_id,
                text: String::new(),
                metadata: Metadata::new(),
            },
            chunks,
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn measure(index: &ExactVectorIndex, scenario: Scenario) -> Result<ScenarioReport, String> {
    for _ in 0..WARMUPS {
        execute(index, &scenario)?;
    }
    let mut raw_duration_ns = Vec::with_capacity(SAMPLES);
    let mut final_identities = Vec::new();
    let mut deleted_hits = 0;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let identities = execute(index, &scenario)?;
        raw_duration_ns.push(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        deleted_hits += identities
            .iter()
            .filter(|identity| identity.starts_with("deleted-"))
            .count();
        final_identities = identities;
    }
    let expected = record_id(TARGET_RECORD, false);
    let observed = final_identities.first().cloned().unwrap_or_default();
    if observed != expected {
        return Err(format!(
            "top document mismatch: expected {expected}, observed {observed}"
        ));
    }
    if deleted_hits != 0 {
        return Err("deleted document appeared in graph-free results".to_owned());
    }
    let identity_hash = sha256_hex(final_identities.join("\n").as_bytes());
    let mut sorted = raw_duration_ns.clone();
    sorted.sort_unstable();
    Ok(ScenarioReport {
        scenario: match scenario {
            Scenario::Semantic => "semantic_exact_vector",
            Scenario::Bm25 => "bm25_internal",
            Scenario::Hybrid => "hybrid_weighted_normalized_0.6_0.4",
        },
        p50_ns: nearest_rank(&sorted, 50),
        p95_ns: nearest_rank(&sorted, 95),
        p99_ns: nearest_rank(&sorted, 99),
        raw_duration_ns,
        result_identity_sha256: identity_hash,
        expected_top_document_id: expected,
        observed_top_document_id: observed,
        deleted_hits,
    })
}

fn execute(index: &ExactVectorIndex, scenario: &Scenario) -> Result<Vec<String>, String> {
    let embedding = source_embedding(TARGET_RECORD, 0, false);
    let text = format!("identity{TARGET_RECORD:08} section00");
    match scenario {
        Scenario::Semantic => index
            .search(&SearchQuery::new(embedding, TOP_K))
            .map(|hits| hits.into_iter().map(|hit| hit.document_id).collect()),
        Scenario::Bm25 => index
            .keyword_search(&KeywordQuery::new(text, TOP_K))
            .map(|hits| hits.into_iter().map(|hit| hit.document_id).collect()),
        Scenario::Hybrid => index
            .hybrid_search(
                &HybridQuery::new(text, embedding, TOP_K).with_weighted_normalized_score(0.6, 0.4),
            )
            .map(|hits| hits.into_iter().map(|hit| hit.document_id).collect()),
    }
    .map_err(|error| error.to_string())
}

fn record_id(index: usize, deleted: bool) -> String {
    if deleted {
        format!("deleted-{index:08}")
    } else {
        format!("record-{index:08}")
    }
}

fn chunk_text(record_index: usize, chunk_index: usize, deleted: bool) -> String {
    let state = if deleted { "deleted" } else { "active" };
    format!(
        "{state} identity{record_index:08} section{chunk_index:02} tenant{} category{} deterministic local retrieval distractor{}",
        record_index % 4,
        (record_index * CHUNKS_PER_RECORD + chunk_index) % 8,
        record_index ^ 0x55aa
    )
}

fn source_embedding(record_index: usize, chunk_index: usize, deleted: bool) -> Vec<f32> {
    let vector_id = ((record_index as u64) << 3) | ((chunk_index as u64) << 1) | u64::from(deleted);
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ vector_id.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let mut values = Vec::with_capacity(DIMENSION);
    for _ in 0..DIMENSION {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let bits = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        values.push(((bits >> 40) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0);
    }
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    values.iter_mut().for_each(|value| *value /= norm);
    values
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100).max(1);
    sorted[rank.saturating_sub(1).min(sorted.len().saturating_sub(1))]
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn failure(message: &str) -> Response {
    Response {
        ok: false,
        report: None,
        error: Some(message.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{nearest_rank, source_embedding, DIMENSION};

    #[test]
    fn source_embedding_is_stable_and_normalized() {
        let first = source_embedding(4, 0, false);
        assert_eq!(first, source_embedding(4, 0, false));
        assert_eq!(first.len(), DIMENSION);
        let norm = first.iter().map(|value| value * value).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.000_01);
    }

    #[test]
    fn percentile_is_one_based_nearest_rank() {
        let values = (1..=100).collect::<Vec<_>>();
        assert_eq!(nearest_rank(&values, 50), 50);
        assert_eq!(nearest_rank(&values, 95), 95);
        assert_eq!(nearest_rank(&values, 99), 99);
    }
}
