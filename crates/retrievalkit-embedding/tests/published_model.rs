use std::{
    env,
    time::{Duration, Instant},
};

use retrievalkit_embedding::{
    DownloadPolicy, EmbeddingError, EmbeddingProfile, OnnxTextEmbedder, TextEmbedder,
    EMBEDDING_DIMENSION,
};

fn selected_profile() -> EmbeddingProfile {
    match env::var("RETRIEVALKIT_EMBEDDING_TEST_PROFILE")
        .unwrap_or_else(|_| "fp32".to_owned())
        .as_str()
    {
        "fp32" => EmbeddingProfile::Fp32,
        "fp16" => EmbeddingProfile::Fp16,
        "q8" => EmbeddingProfile::Q8,
        value => panic!("unsupported RETRIEVALKIT_EMBEDDING_TEST_PROFILE: {value}"),
    }
}

fn assert_valid_embedding(embedding: &[f32]) {
    assert_eq!(embedding.len(), EMBEDDING_DIMENSION);
    assert!(embedding.iter().all(|value| value.is_finite()));
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    assert!((norm - 1.0).abs() < 1e-4, "unexpected L2 norm: {norm}");
}

/// Downloads the selected immutable public profile and runs real ONNX Runtime
/// inference. Kept ignored so normal test runs remain network-free.
#[test]
#[ignore = "downloads a pinned public model artifact"]
fn published_profile_downloads_then_runs_from_verified_cache() {
    let profile = selected_profile();

    let cold_started = Instant::now();
    let embedder = OnnxTextEmbedder::builder()
        .profile(profile)
        .build()
        .expect("pinned public profile should download and load");
    let cold_load = cold_started.elapsed();
    let first_started = Instant::now();
    let first = embedder
        .embed("RetrievalKit keeps retrieval local and deterministic.")
        .expect("first embedding should succeed");
    let first_inference = first_started.elapsed();
    assert_valid_embedding(&first);
    assert_valid_embedding(
        &embedder
            .embed("İstanbul'da çevrimdışı arama 東京")
            .expect("Unicode embedding should succeed"),
    );
    assert!(matches!(
        embedder.embed(""),
        Err(EmbeddingError::EmptyInput)
    ));

    let cached_started = Instant::now();
    let cached = OnnxTextEmbedder::builder()
        .profile(profile)
        .download_policy(DownloadPolicy::LocalOnly)
        .build()
        .expect("verified cache should load without network");
    let cached_load = cached_started.elapsed();
    let warm_started = Instant::now();
    let warm = cached
        .embed("RetrievalKit keeps retrieval local and deterministic.")
        .expect("cached embedding should succeed");
    let warm_inference = warm_started.elapsed();
    assert_valid_embedding(&warm);

    let cosine = first
        .iter()
        .zip(&warm)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    assert!(cosine >= 0.999_999, "cached output drifted: {cosine}");

    eprintln!(
        "profile={profile} cold_load_ms={:.3} first_inference_ms={:.3} cached_load_ms={:.3} warm_inference_ms={:.3}",
        milliseconds(cold_load),
        milliseconds(first_inference),
        milliseconds(cached_load),
        milliseconds(warm_inference),
    );
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
