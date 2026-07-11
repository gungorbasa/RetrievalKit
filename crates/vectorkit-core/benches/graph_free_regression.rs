use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use vectorkit_core::{
    ChunkInput, Document, ExactVectorIndex, HybridQuery, IndexConfig, KeywordQuery, Metadata,
    MetadataValue, SearchQuery, VectorEncoding, VectorMetric,
};

const CHUNKS: usize = 10_000;
const DIMENSION: usize = 384;
const WARMUP: usize = 100;
const SAMPLES: usize = 1_000;

fn main() {
    let index = build_index();
    let embedding = query_embedding();
    let vector = SearchQuery::new(embedding.clone(), 10);
    let keyword = KeywordQuery::new("shared topic17", 10);
    let hybrid = HybridQuery::new("shared topic17", embedding, 10);

    let vector_p95 = measure(|| index.search(black_box(&vector)).unwrap());
    let keyword_p95 = measure(|| index.keyword_search(black_box(&keyword)).unwrap());
    let hybrid_p95 = measure(|| index.hybrid_search(black_box(&hybrid)).unwrap());

    println!(
        "{{\"chunks\":{CHUNKS},\"dimension\":{DIMENSION},\"top_k\":10,\"encoding\":\"f32\",\"build_mode\":\"release\",\"warmup\":{WARMUP},\"samples\":{SAMPLES},\"percentile\":\"nearest-rank-ceil\",\"embedding_excluded\":true,\"exact_p95_us\":{},\"bm25_p95_us\":{},\"hybrid_p95_us\":{}}}",
        vector_p95.as_micros(),
        keyword_p95.as_micros(),
        hybrid_p95.as_micros()
    );
}

fn build_index() -> ExactVectorIndex {
    let config = IndexConfig::new(DIMENSION, VectorMetric::DotProduct)
        .with_vector_encoding(VectorEncoding::F32);
    let mut index = ExactVectorIndex::try_with_config(config).unwrap();
    for ordinal in 0..CHUNKS {
        index
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
    }
    index
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

fn query_embedding() -> Vec<f32> {
    embedding_for(7_777)
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
