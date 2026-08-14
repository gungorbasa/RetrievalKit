use std::collections::BTreeMap;
use std::env;
use std::hint::black_box;
use std::time::Duration;

use retrievalkit_core::{
    ChunkInput, Document, ExactVectorIndex, Filter, HybridFusionTrace, HybridHit, HybridQuery,
    HybridStageDurations, IndexConfig, Metadata, MetadataValue, VectorEncoding, VectorMetric,
};
use serde_json::{json, Value};

const DIMENSION: usize = 384;
const TOP_K: usize = 10;
const VECTOR_CANDIDATES: usize = 50;
const KEYWORD_CANDIDATES: usize = 50;
const DEFAULT_WARMUP: usize = 20;
const DEFAULT_SAMPLES: usize = 200;
const DOMAINS: [&str; 12] = [
    "account security",
    "offline notes",
    "photo organization",
    "travel planning",
    "expense tracking",
    "team projects",
    "customer support",
    "device setup",
    "health records",
    "course materials",
    "legal documents",
    "home inventory",
];
const LOCATIONS: [&str; 8] = [
    "Ankara", "Berlin", "Boston", "London", "Paris", "Seattle", "Tokyo", "Toronto",
];
const TEAMS: [&str; 8] = [
    "Atlas", "Beacon", "Cedar", "Delta", "Ember", "Falcon", "Harbor", "Juniper",
];
const STATES: [&str; 4] = ["draft", "active", "review", "archived"];
const CHUNK_KEYS: [&str; 4] = ["overview", "procedure", "diagnostics", "reference"];

fn main() {
    let chunk_counts = env::var("RETRIEVALKIT_PROFILE_CHUNKS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|part| part.parse::<usize>().expect("invalid chunk count"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![25_000, 49_999]);
    let warmup = env_usize("RETRIEVALKIT_PROFILE_WARMUP", DEFAULT_WARMUP);
    let samples = env_usize("RETRIEVALKIT_PROFILE_SAMPLES", DEFAULT_SAMPLES);
    assert!(
        samples > 0,
        "RETRIEVALKIT_PROFILE_SAMPLES must be greater than zero"
    );

    let workloads = chunk_counts
        .into_iter()
        .map(|chunks| profile_workload(chunks, warmup, samples))
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema_version": 2,
            "benchmark": "hybrid_stage_profile",
            "dimension": DIMENSION,
            "encoding": "i8_scalar_quantized",
            "metric": "cosine",
            "top_k": TOP_K,
            "vector_candidates": VECTOR_CANDIDATES,
            "keyword_candidates": KEYWORD_CANDIDATES,
            "warmup": warmup,
            "samples": samples,
            "percentile": "nearest-rank-ceil",
            "embedding_excluded": true,
            "workloads": workloads,
        }))
        .expect("serialize benchmark report")
    );
}

fn profile_workload(chunks: usize, warmup: usize, samples: usize) -> Value {
    assert!(
        chunks >= TOP_K,
        "workload must contain at least top_k chunks"
    );
    let index = build_index(chunks);
    let queries = benchmark_queries();
    let scenarios = [
        ("unfiltered", None),
        ("team_atlas_filter", Some(Filter::eq("team", "Atlas"))),
    ];

    let scenario_reports = scenarios
        .into_iter()
        .map(|(name, filter)| {
            let mut query_ordinal = 0usize;
            let mut run = || {
                let query_index = query_ordinal % queries.len();
                let (text, embedding_seed) = &queries[query_index];
                query_ordinal += 1;
                let mut query = HybridQuery::new(text, embedding_for(*embedding_seed), TOP_K)
                    .with_candidate_limits(VECTOR_CANDIDATES, KEYWORD_CANDIDATES)
                    .with_alpha(0.6);
                if let Some(filter) = &filter {
                    query = query.with_filter(filter.clone());
                }
                (
                    query_index,
                    index.profile_hybrid_search(black_box(&query)).unwrap(),
                )
            };

            for _ in 0..warmup {
                black_box(run());
            }

            let mut stage_samples = StageSamples::with_capacity(samples);
            let mut result_digest = 1_469_598_103_934_665_603u64;
            for _ in 0..samples {
                let (query_index, profile) = black_box(run());
                stage_samples.push(profile.stages);
                result_digest = digest_u64(result_digest, query_index as u64);
                result_digest = digest_u64(result_digest, profile.hits.len() as u64);
                for hit in &profile.hits {
                    result_digest = digest_hybrid_hit(result_digest, hit);
                }
            }

            json!({
                "name": name,
                "result_digest": format!("{result_digest:016x}"),
                "stages": stage_samples.report(),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "active_chunks": index.active_chunk_count(),
        "requested_chunks": chunks,
        "query_count": queries.len(),
        "corpus": "apple-e2e-realistic-text-v1",
        "scenarios": scenario_reports,
    })
}

fn build_index(chunks: usize) -> ExactVectorIndex {
    let config = IndexConfig::new(DIMENSION, VectorMetric::Cosine)
        .with_vector_encoding(VectorEncoding::I8ScalarQuantized);
    let mut index = ExactVectorIndex::try_with_config(config).expect("create index");
    let mut remaining = chunks;
    let mut record_number = 0usize;
    while remaining > 0 {
        let chunk_count = remaining.min(CHUNK_KEYS.len());
        let team = TEAMS[(record_number / 3) % TEAMS.len()];
        let chunks = (0..chunk_count)
            .map(|chunk_number| ChunkInput {
                text: chunk_text(record_number, chunk_number),
                embedding: embedding_for(record_number * CHUNK_KEYS.len() + chunk_number),
                metadata: Metadata::new(),
            })
            .collect();
        index
            .upsert_document(
                Document {
                    id: format!("record-{record_number:05}"),
                    text: format!("Local application record {record_number:05}"),
                    metadata: BTreeMap::from([
                        (
                            "domain".to_owned(),
                            MetadataValue::String(
                                DOMAINS[record_number % DOMAINS.len()].to_owned(),
                            ),
                        ),
                        (
                            "state".to_owned(),
                            MetadataValue::String(
                                STATES[(record_number / 7) % STATES.len()].to_owned(),
                            ),
                        ),
                        ("team".to_owned(), MetadataValue::String(team.to_owned())),
                    ]),
                },
                chunks,
            )
            .expect("upsert benchmark record");
        remaining -= chunk_count;
        record_number += 1;
    }
    assert_eq!(index.active_chunk_count(), chunks);
    index
}

fn chunk_text(record_number: usize, chunk_number: usize) -> String {
    let domain = DOMAINS[record_number % DOMAINS.len()];
    let location = LOCATIONS[(record_number / DOMAINS.len()) % LOCATIONS.len()];
    let team = TEAMS[(record_number / 3) % TEAMS.len()];
    let state = STATES[(record_number / 7) % STATES.len()];
    let identifier = format!("RK-{record_number:05}-{}", chunk_number + 1);
    let key = CHUNK_KEYS[chunk_number];
    match chunk_number {
        0 => format!(
            "This {key} explains the {domain} workspace owned by team {team} in {location}. \
             The item is {state} and its reference is {identifier}. It summarizes goals, owners, \
             important dates, and the information a person usually needs when searching the app."
        ),
        1 => format!(
            "Use this {key} when completing a {domain} task. Open the local workspace, confirm team \
             {team}, select the {location} collection, and follow reference {identifier}. The steps \
             include verification, a safe fallback, and the expected completion state."
        ),
        2 => format!(
            "Troubleshooting for {domain}: if the {state} item cannot be found, check spelling, the \
             team {team} filter, the {location} collection, and identifier {identifier}. Review the \
             offline copy before changing or deleting any stored information."
        ),
        3 => format!(
            "Reference details for {identifier}. Domain: {domain}. Team: {team}. Location: {location}. \
             State: {state}. This entry contains searchable names, exact identifiers, related terms, \
             and enough surrounding text to hydrate a realistic local search result."
        ),
        _ => unreachable!(),
    }
}

fn benchmark_queries() -> Vec<(String, usize)> {
    [
        ("find the offline instructions about account security for the Atlas group in Ankara", 7),
        ("RK-00017-4 Falcon Berlin", 17),
        ("how do I troubleshoot travel planning RK-00039-3 for team Falcon", 39),
        (
            "find local document information project result details search local document \
             information project result details search local document information project result \
             details for offline notes team Atlas in Toronto",
            64,
        ),
        (
            "search local document information project result details search local document \
             information project result details search local document information project result \
             details search local document information project result details search local document \
             information project result details search local document information project result \
             details search local document information project result details about device setup \
             for team Cedar in Boston",
            91,
        ),
    ]
    .into_iter()
    .map(|(text, seed)| (text.to_owned(), seed))
    .collect()
}

fn embedding_for(seed: usize) -> Vec<f32> {
    (0..DIMENSION)
        .map(|dimension| {
            let value = seed
                .wrapping_mul(1_664_525)
                .wrapping_add(dimension.wrapping_mul(1_013_904_223));
            (value % 2_001) as f32 / 1_000.0 - 1.0
        })
        .collect()
}

struct StageSamples {
    filter: Vec<Duration>,
    vector: Vec<Duration>,
    bm25: Vec<Duration>,
    fusion: Vec<Duration>,
    hydration: Vec<Duration>,
    total: Vec<Duration>,
}

impl StageSamples {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            filter: Vec::with_capacity(capacity),
            vector: Vec::with_capacity(capacity),
            bm25: Vec::with_capacity(capacity),
            fusion: Vec::with_capacity(capacity),
            hydration: Vec::with_capacity(capacity),
            total: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, stages: HybridStageDurations) {
        self.filter.push(stages.filter);
        self.vector.push(stages.vector);
        self.bm25.push(stages.bm25);
        self.fusion.push(stages.fusion);
        self.hydration.push(stages.hydration);
        self.total.push(stages.total);
    }

    fn report(mut self) -> Value {
        json!({
            "filter": summarize(&mut self.filter),
            "vector": summarize(&mut self.vector),
            "bm25": summarize(&mut self.bm25),
            "fusion": summarize(&mut self.fusion),
            "hydration": summarize(&mut self.hydration),
            "total": summarize(&mut self.total),
        })
    }
}

fn summarize(samples: &mut [Duration]) -> Value {
    samples.sort_unstable();
    let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];
    let total_ns = samples.iter().map(Duration::as_nanos).sum::<u128>();
    json!({
        "mean_ns": total_ns / samples.len() as u128,
        "p95_ns": p95.as_nanos(),
    })
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .map(|value| value.parse::<usize>().expect("invalid positive integer"))
        .unwrap_or(default)
}

fn digest_hybrid_hit(mut digest: u64, hit: &HybridHit) -> u64 {
    digest = digest_u64(digest, hit.chunk_id);
    digest = digest_string(digest, &hit.document_id);
    digest = digest_u64(digest, hit.score.to_bits() as u64);
    digest = digest_optional_f32(digest, hit.vector_score);
    digest = digest_optional_f32(digest, hit.keyword_score);
    digest = digest_optional_usize(digest, hit.trace.vector_rank);
    digest = digest_optional_usize(digest, hit.trace.keyword_rank);
    digest = digest_optional_f32(digest, hit.trace.normalized_vector_score);
    digest = digest_optional_f32(digest, hit.trace.normalized_keyword_score);
    digest = digest_u64(digest, hit.trace.matched_terms.len() as u64);
    for term in &hit.trace.matched_terms {
        digest = digest_string(digest, term);
    }
    match hit.trace.fusion {
        HybridFusionTrace::ReciprocalRank { rrf_k } => {
            digest = digest_u64(digest, 0);
            digest_u64(digest, rrf_k.to_bits() as u64)
        }
        HybridFusionTrace::WeightedNormalizedScore {
            vector_weight,
            keyword_weight,
        } => {
            digest = digest_u64(digest, 1);
            digest = digest_u64(digest, vector_weight.to_bits() as u64);
            digest_u64(digest, keyword_weight.to_bits() as u64)
        }
    }
}

fn digest_optional_f32(mut digest: u64, value: Option<f32>) -> u64 {
    match value {
        Some(value) => {
            digest = digest_u64(digest, 1);
            digest_u64(digest, value.to_bits() as u64)
        }
        None => digest_u64(digest, 0),
    }
}

fn digest_optional_usize(mut digest: u64, value: Option<usize>) -> u64 {
    match value {
        Some(value) => {
            digest = digest_u64(digest, 1);
            digest_u64(digest, value as u64)
        }
        None => digest_u64(digest, 0),
    }
}

fn digest_string(mut digest: u64, value: &str) -> u64 {
    digest = digest_u64(digest, value.len() as u64);
    for byte in value.as_bytes() {
        digest ^= u64::from(*byte);
        digest = digest.wrapping_mul(1_099_511_628_211);
    }
    digest
}

fn digest_u64(mut digest: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(1_099_511_628_211);
    }
    digest
}
