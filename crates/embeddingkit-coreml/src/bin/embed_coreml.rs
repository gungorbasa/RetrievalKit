use coreml_native::ComputeUnits;
use embeddingkit_coreml::{CoreMlEmbeddingConfig, CoreMlTextEmbedder};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

const DEFAULT_SEQUENCE_LENGTH: usize = 256;
const DEFAULT_DIMENSION: usize = 384;
const DEFAULT_OUTPUT_NAME: &str = "embedding";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse(std::env::args().skip(1))?;
    let config = CoreMlEmbeddingConfig::new(options.model_path, options.tokenizer_path)
        .with_sequence_length(options.sequence_length)
        .with_output_name(options.output_name)
        .with_compute_units(options.compute_units);
    let embedder = CoreMlTextEmbedder::load(config)?;

    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut output = vec![0.0f32; options.dimension];

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: EmbedRequest = serde_json::from_str(&line)?;
        let count = embedder.embed_into(&request.text, &mut output)?;
        let response = EmbedResponse {
            embedding: &output[..count],
        };
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }

    Ok(())
}

#[derive(Debug)]
struct Options {
    model_path: PathBuf,
    tokenizer_path: PathBuf,
    sequence_length: usize,
    dimension: usize,
    output_name: String,
    compute_units: ComputeUnits,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut model_dir: Option<PathBuf> = None;
        let mut model_path: Option<PathBuf> = None;
        let mut tokenizer_path: Option<PathBuf> = None;
        let mut sequence_length = DEFAULT_SEQUENCE_LENGTH;
        let mut dimension = DEFAULT_DIMENSION;
        let mut output_name = DEFAULT_OUTPUT_NAME.to_string();
        let mut compute_units = ComputeUnits::CpuAndNeuralEngine;

        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--model-dir" => {
                    let value = next_value(&mut args, "--model-dir")?;
                    model_dir = Some(PathBuf::from(value));
                }
                "--model-path" => {
                    let value = next_value(&mut args, "--model-path")?;
                    model_path = Some(PathBuf::from(value));
                }
                "--tokenizer-path" => {
                    let value = next_value(&mut args, "--tokenizer-path")?;
                    tokenizer_path = Some(PathBuf::from(value));
                }
                "--sequence-length" => {
                    sequence_length = next_value(&mut args, "--sequence-length")?.parse()?;
                }
                "--dimension" => {
                    dimension = next_value(&mut args, "--dimension")?.parse()?;
                }
                "--output-name" => {
                    output_name = next_value(&mut args, "--output-name")?;
                }
                "--compute" => {
                    compute_units = parse_compute_units(&next_value(&mut args, "--compute")?)?;
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }

        if let Some(model_dir) = model_dir.as_deref() {
            if model_path.is_none() {
                model_path = Some(model_dir.join("AllMiniLML6V2.mlmodelc"));
            }
            if tokenizer_path.is_none() {
                tokenizer_path = Some(model_dir.join("tokenizer/tokenizer.json"));
            }

            if let Ok(metadata) = ModelMetadata::load(&model_dir.join("metadata.json")) {
                sequence_length = metadata.sequence_length.unwrap_or(sequence_length);
                dimension = metadata.dimension.unwrap_or(dimension);
                output_name = metadata.output.unwrap_or(output_name);
            }
        }

        let model_path = model_path.ok_or("missing --model-dir or --model-path")?;
        let tokenizer_path = tokenizer_path.ok_or("missing --model-dir or --tokenizer-path")?;

        Ok(Self {
            model_path,
            tokenizer_path,
            sequence_length,
            dimension,
            output_name,
            compute_units,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ModelMetadata {
    sequence_length: Option<usize>,
    dimension: Option<usize>,
    output: Option<String>,
}

impl ModelMetadata {
    fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let data = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    }
}

#[derive(Debug, Deserialize)]
struct EmbedRequest {
    text: String,
}

#[derive(Debug, Serialize)]
struct EmbedResponse<'a> {
    embedding: &'a [f32],
}

fn next_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}").into())
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

fn print_usage() {
    eprintln!(
        "usage: embeddingkit-coreml-embed --model-dir <dir> [--compute cpuAndNeuralEngine]\n\
         Reads JSONL from stdin: {{\"text\":\"...\"}}\n\
         Writes JSONL to stdout: {{\"embedding\":[...]}}"
    );
}
