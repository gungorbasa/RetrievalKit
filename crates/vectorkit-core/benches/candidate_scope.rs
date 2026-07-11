use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use vectorkit_core::{
    ChunkId, ChunkInput, Document, ExactVectorIndex, HybridQuery, IndexConfig, KeywordQuery,
    Metadata, MetadataValue, SearchQuery, VectorEncoding, VectorMetric,
};

const CHUNKS: usize = 10_000;
const DIMENSION: usize = 384;
const WARMUP: usize = 100;
const SAMPLES: usize = 500;

fn main() {
    let (index, ids) = build_index();
    let sparse = index
        .candidate_scope(ids.iter().copied().step_by(100))
        .unwrap();
    let dense = index
        .candidate_scope(ids.iter().copied().filter(|id| id % 2 == 0))
        .unwrap();
    let embedding = embedding_for(7_777);
    let exact_query = SearchQuery::new(embedding.clone(), 10);
    let keyword_query = KeywordQuery::new("shared topic17", 10);
    let hybrid_query = HybridQuery::new("shared topic17", embedding, 10);

    let sparse_exact = measure(|| {
        index
            .search_in_candidates(black_box(&exact_query), black_box(&sparse))
            .unwrap()
    });
    let dense_exact = measure(|| {
        index
            .search_in_candidates(black_box(&exact_query), black_box(&dense))
            .unwrap()
    });
    let sparse_bm25 = measure(|| {
        index
            .keyword_search_in_candidates(black_box(&keyword_query), black_box(&sparse))
            .unwrap()
    });
    let dense_bm25 = measure(|| {
        index
            .keyword_search_in_candidates(black_box(&keyword_query), black_box(&dense))
            .unwrap()
    });
    let sparse_hybrid = measure(|| {
        index
            .hybrid_search_in_candidates(black_box(&hybrid_query), black_box(&sparse))
            .unwrap()
    });
    let dense_hybrid = measure(|| {
        index
            .hybrid_search_in_candidates(black_box(&hybrid_query), black_box(&dense))
            .unwrap()
    });

    println!(
        "{{\"chunks\":{CHUNKS},\"dimension\":{DIMENSION},\"build_mode\":\"release\",\"warmup\":{WARMUP},\"samples\":{SAMPLES},\"sparse_candidates\":{},\"dense_candidates\":{},\"sparse_exact_p95_us\":{},\"dense_exact_p95_us\":{},\"sparse_bm25_p95_us\":{},\"dense_bm25_p95_us\":{},\"sparse_hybrid_p95_us\":{},\"dense_hybrid_p95_us\":{}}}",
        sparse.len(),
        dense.len(),
        sparse_exact.as_micros(),
        dense_exact.as_micros(),
        sparse_bm25.as_micros(),
        dense_bm25.as_micros(),
        sparse_hybrid.as_micros(),
        dense_hybrid.as_micros()
    );
}

fn build_index() -> (ExactVectorIndex, Vec<ChunkId>) {
    let config = IndexConfig::new(DIMENSION, VectorMetric::DotProduct)
        .with_vector_encoding(VectorEncoding::F32);
    let mut index = ExactVectorIndex::try_with_config(config).unwrap();
    let mut ids = Vec::with_capacity(CHUNKS);
    for ordinal in 0..CHUNKS {
        let chunk_ids = index
            .upsert_document(
                Document {
                    id: format!("record-{ordinal}"),
                    text: String::new(),
                    metadata: BTreeMap::from([(
                        "partition".to_owned(),
                        MetadataValue::Integer((ordinal % 100) as i64),
                    )]),
                },
                vec![ChunkInput {
                    text: format!("shared topic{} record{ordinal}", ordinal % 64),
                    embedding: embedding_for(ordinal),
                    metadata: Metadata::new(),
                }],
            )
            .unwrap();
        ids.push(chunk_ids[0]);
    }
    (index, ids)
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

fn measure<T>(mut operation: impl FnMut() -> T) -> Duration {
    for _ in 0..WARMUP {
        black_box(operation());
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        black_box(operation());
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    samples[(SAMPLES * 95).div_ceil(100) - 1]
}
