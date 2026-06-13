use coreml_native::ComputeUnits;
use embeddingkit_coreml::{CoreMlEmbeddingConfig, CoreMlTextEmbedder};
use std::path::Path;
use std::time::Instant;

const DIMENSION: usize = 384;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model_path = repo.join("target/embedding-models/all-MiniLM-L6-v2/AllMiniLML6V2.mlmodelc");
    let tokenizer_path =
        repo.join("target/embedding-models/all-MiniLM-L6-v2/tokenizer/tokenizer.json");
    let mut args = std::env::args().skip(1);
    let compute_units = args
        .next()
        .as_deref()
        .map(parse_compute_units)
        .transpose()?
        .unwrap_or(ComputeUnits::All);
    let text = args
        .next()
        .unwrap_or_else(|| "Mark and Erica arguing at the party".to_string());

    let load_start = Instant::now();
    let embedder = CoreMlTextEmbedder::load(
        CoreMlEmbeddingConfig::new(model_path, tokenizer_path).with_compute_units(compute_units),
    )?;
    let load_ms = load_start.elapsed().as_secs_f64() * 1_000.0;

    let mut embedding = vec![0.0f32; DIMENSION];
    let first_start = Instant::now();
    let count = embedder.embed_into(&text, &mut embedding)?;
    let first_ms = first_start.elapsed().as_secs_f64() * 1_000.0;

    let mut measured_ms = Vec::with_capacity(750);
    for index in 0..800 {
        let start = Instant::now();
        embedder.embed_into(&text, &mut embedding)?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0;
        if index >= 50 {
            measured_ms.push(elapsed_ms);
        }
    }
    measured_ms.sort_by(|left, right| left.total_cmp(right));

    let mean = measured_ms.iter().sum::<f64>() / measured_ms.len() as f64;
    let p50 = percentile(&measured_ms, 0.50);
    let p95 = percentile(&measured_ms, 0.95);
    let p99 = percentile(&measured_ms, 0.99);
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();

    println!("model_load_ms={load_ms:.3}");
    println!("compute_units={compute_units}");
    println!("first_embedding_ms={first_ms:.3}");
    println!("measured_queries={}", measured_ms.len());
    println!("mean_ms={mean:.3}");
    println!("p50_ms={p50:.3}");
    println!("p95_ms={p95:.3}");
    println!("p99_ms={p99:.3}");
    println!("embedding_count={count}");
    println!("embedding_norm={norm:.6}");

    Ok(())
}

fn parse_compute_units(value: &str) -> Result<ComputeUnits, Box<dyn std::error::Error>> {
    match value {
        "all" => Ok(ComputeUnits::All),
        "cpu" | "cpuOnly" => Ok(ComputeUnits::CpuOnly),
        "cpuAndGPU" | "gpu" => Ok(ComputeUnits::CpuAndGpu),
        "cpuAndNeuralEngine" | "neural" | "ane" => Ok(ComputeUnits::CpuAndNeuralEngine),
        other => Err(format!(
            "unknown compute units '{other}', expected all, cpu, cpuAndGPU, or cpuAndNeuralEngine"
        )
        .into()),
    }
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}
