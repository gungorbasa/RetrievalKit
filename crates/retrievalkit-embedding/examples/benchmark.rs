use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    hint::black_box,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};

use retrievalkit_core::{
    ChunkInput, Document, ExactVectorIndex, IndexConfig, Metadata, SearchQuery, VectorEncoding,
    VectorMetric,
};
use retrievalkit_embedding::{DownloadPolicy, EmbeddingProfile, OnnxTextEmbedder, TextEmbedder};

const DIMENSION: usize = 384;
const CHUNKS: usize = 10_000;
const TOKEN_LENGTHS: [usize; 5] = [16, 32, 64, 128, 256];
const BATCH_SIZES: [usize; 3] = [1, 8, 32];

fn main() {
    let profile = parse_profile(env::args().nth(1).as_deref().unwrap_or("fp32"));
    let warmups = parse_count("RETRIEVALKIT_EMBEDDING_BENCH_WARMUPS", 50);
    let samples = parse_count("RETRIEVALKIT_EMBEDDING_BENCH_SAMPLES", 750);
    let intra_threads = parse_count("RETRIEVALKIT_EMBEDDING_BENCH_INTRA_THREADS", 4);
    let token_lengths = parse_list("RETRIEVALKIT_EMBEDDING_BENCH_TOKEN_LENGTHS", &TOKEN_LENGTHS);
    let batch_sizes = parse_list("RETRIEVALKIT_EMBEDDING_BENCH_BATCH_SIZES", &BATCH_SIZES);

    let load_started = Instant::now();
    let embedder = OnnxTextEmbedder::builder()
        .profile(profile)
        .download_policy(DownloadPolicy::LocalOnly)
        .intra_threads(intra_threads)
        .build()
        .expect("the selected profile must already exist in the verified cache");
    let cached_load = load_started.elapsed();

    println!(
        "{{\"kind\":\"configuration\",\"profile\":\"{profile}\",\"dimension\":{DIMENSION},\"chunks\":{CHUNKS},\"warmups\":{warmups},\"samples\":{samples},\"intra_threads\":{intra_threads},\"cached_load_ms\":{:.6}}}",
        milliseconds(cached_load)
    );

    if dump_conformance_vectors(&embedder, profile) {
        return;
    }

    for token_length in token_lengths {
        let text = text_with_token_length(token_length);
        for &batch_size in &batch_sizes {
            let batch = vec![text.as_str(); batch_size];
            let p95 = measure(warmups, samples, || {
                embedder
                    .embed_batch(black_box(&batch))
                    .expect("embedding benchmark inference should succeed")
            });
            println!(
                "{{\"kind\":\"embedding\",\"profile\":\"{profile}\",\"token_length\":{token_length},\"batch_size\":{batch_size},\"p95_ms\":{:.6},\"per_item_p95_ms\":{:.6}}}",
                milliseconds(p95),
                milliseconds(p95) / batch_size as f64
            );
        }
    }

    let index = build_index();
    let query_text = text_with_token_length(32);
    let query_embedding = embedder
        .embed(&query_text)
        .expect("benchmark query embedding should succeed");
    let query = SearchQuery::new(query_embedding, 10);
    let retrieval_p95 = measure(warmups, samples, || {
        index
            .search(black_box(&query))
            .expect("retrieval benchmark should succeed")
    });
    let end_to_end_p95 = measure(warmups, samples, || {
        let embedding = embedder
            .embed(black_box(&query_text))
            .expect("end-to-end embedding should succeed");
        index
            .search(&SearchQuery::new(embedding, 10))
            .expect("end-to-end retrieval should succeed")
    });
    println!(
        "{{\"kind\":\"retrieval\",\"profile\":\"{profile}\",\"token_length\":32,\"batch_size\":1,\"retrieval_p95_ms\":{:.6},\"embedding_plus_retrieval_p95_ms\":{:.6}}}",
        milliseconds(retrieval_p95),
        milliseconds(end_to_end_p95)
    );
}

fn parse_profile(value: &str) -> EmbeddingProfile {
    match value {
        "fp32" => EmbeddingProfile::Fp32,
        "fp16" => EmbeddingProfile::Fp16,
        "q8" => EmbeddingProfile::Q8,
        _ => panic!("profile must be fp32, fp16, or q8"),
    }
}

fn parse_count(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .map(|value| value.parse().expect("benchmark count must be an integer"))
        .unwrap_or(default)
}

fn parse_list(name: &str, default: &[usize]) -> Vec<usize> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|item| item.parse().expect("benchmark list items must be integers"))
                .collect()
        })
        .unwrap_or_else(|| default.to_vec())
}

fn text_with_token_length(token_length: usize) -> String {
    assert!((2..=256).contains(&token_length));
    std::iter::repeat_n("hello", token_length - 2)
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_index() -> ExactVectorIndex {
    let config = IndexConfig::new(DIMENSION, VectorMetric::DotProduct)
        .with_vector_encoding(VectorEncoding::I8ScalarQuantized);
    let mut index = ExactVectorIndex::try_with_config(config).expect("valid benchmark index");
    for ordinal in 0..CHUNKS {
        index
            .upsert_document(
                Document {
                    id: format!("record-{ordinal}"),
                    text: String::new(),
                    metadata: BTreeMap::new(),
                },
                vec![ChunkInput {
                    text: format!("local retrieval benchmark record {ordinal}"),
                    embedding: deterministic_embedding(ordinal),
                    metadata: Metadata::new(),
                }],
            )
            .expect("benchmark insertion should succeed");
    }
    index
}

fn deterministic_embedding(seed: usize) -> Vec<f32> {
    let mut values = (0..DIMENSION)
        .map(|dimension| {
            let value = seed
                .wrapping_mul(1_664_525)
                .wrapping_add(dimension.wrapping_mul(1_013_904_223));
            (value % 2_001) as f32 / 1_000.0 - 1.0
        })
        .collect::<Vec<_>>();
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    for value in &mut values {
        *value /= norm;
    }
    values
}

fn measure<T>(warmups: usize, samples: usize, mut operation: impl FnMut() -> T) -> Duration {
    assert!(samples > 0);
    for _ in 0..warmups {
        black_box(operation());
    }
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        black_box(operation());
        durations.push(started.elapsed());
    }
    durations.sort_unstable();
    durations[(samples * 95).div_ceil(100) - 1]
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn dump_conformance_vectors(embedder: &OnnxTextEmbedder, profile: EmbeddingProfile) -> bool {
    let Ok(input_path) = env::var("RETRIEVALKIT_EMBEDDING_CONFORMANCE_INPUT") else {
        return false;
    };
    let output_path = env::var("RETRIEVALKIT_EMBEDDING_CONFORMANCE_OUTPUT")
        .expect("conformance output path is required when an input path is configured");
    let reader = BufReader::new(File::open(&input_path).expect("conformance input must open"));
    let texts: Vec<String> =
        serde_json::from_reader(reader).expect("conformance input must be a JSON string array");
    let mut embeddings = Vec::with_capacity(texts.len());
    for batch in texts.chunks(32) {
        let borrowed = batch.iter().map(String::as_str).collect::<Vec<_>>();
        embeddings.extend(
            embedder
                .embed_batch(&borrowed)
                .expect("conformance embedding must succeed"),
        );
    }
    let output = Path::new(&output_path);
    fs::create_dir_all(
        output
            .parent()
            .expect("conformance output must have a parent"),
    )
    .expect("conformance output directory must be created");
    serde_json::to_writer(
        File::create(output).expect("conformance output must be created"),
        &embeddings,
    )
    .expect("conformance vectors must serialize");
    println!(
        "{{\"kind\":\"conformance\",\"profile\":\"{profile}\",\"embedding_count\":{},\"output\":\"{}\"}}",
        embeddings.len(),
        output.display()
    );
    env::var("RETRIEVALKIT_EMBEDDING_CONFORMANCE_ONLY").as_deref() == Ok("1")
}
