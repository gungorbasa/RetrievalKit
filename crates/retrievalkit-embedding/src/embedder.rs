use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Tensor,
};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams, TruncationStrategy};

use crate::{
    DownloadPolicy, EmbeddingError, EmbeddingModelInfo, EmbeddingProfile, ModelStore, Result,
    EMBEDDING_DIMENSION, MAX_INPUT_TOKENS,
};

/// Provider-neutral synchronous text embedding boundary.
pub trait TextEmbedder: Send + Sync {
    fn model_info(&self) -> &EmbeddingModelInfo;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

/// Builder for an ONNX Runtime-backed text embedder.
#[derive(Debug)]
pub struct OnnxTextEmbedderBuilder {
    profile: EmbeddingProfile,
    store: Option<ModelStore>,
    cache_dir: Option<std::path::PathBuf>,
    download_policy: DownloadPolicy,
    runtime_library_path: Option<PathBuf>,
    intra_threads: usize,
    inter_threads: usize,
    warmup: bool,
}

impl Default for OnnxTextEmbedderBuilder {
    fn default() -> Self {
        let intra_threads = std::thread::available_parallelism()
            .map(|count| count.get().min(4))
            .unwrap_or(1);
        Self {
            profile: EmbeddingProfile::default(),
            store: None,
            cache_dir: None,
            download_policy: DownloadPolicy::default(),
            runtime_library_path: None,
            intra_threads,
            inter_threads: 1,
            warmup: true,
        }
    }
}

impl OnnxTextEmbedderBuilder {
    pub fn profile(mut self, profile: EmbeddingProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn model_store(mut self, store: ModelStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn cache_dir(mut self, cache_dir: impl Into<std::path::PathBuf>) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self
    }

    pub fn download_policy(mut self, policy: DownloadPolicy) -> Self {
        self.download_policy = policy;
        self
    }

    /// Selects the application-bundled official ONNX Runtime 1.24.3 library.
    ///
    /// If omitted, `RETRIEVALKIT_ONNX_RUNTIME_LIBRARY` must contain the path.
    pub fn runtime_library_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.runtime_library_path = Some(path.into());
        self
    }

    pub fn intra_threads(mut self, threads: usize) -> Self {
        self.intra_threads = threads.max(1);
        self
    }

    pub fn inter_threads(mut self, threads: usize) -> Self {
        self.inter_threads = threads.max(1);
        self
    }

    /// Disables the default single inference warmup, primarily for diagnostics.
    pub fn without_warmup(mut self) -> Self {
        self.warmup = false;
        self
    }

    pub fn build(self) -> Result<OnnxTextEmbedder> {
        initialize_runtime(self.runtime_library_path.as_deref())?;
        let store = match self.store {
            Some(store) => store,
            None => match self.cache_dir {
                Some(cache_dir) => ModelStore::with_cache_dir(cache_dir, self.download_policy)?,
                None => ModelStore::new(self.download_policy)?,
            },
        };
        // This is the only operation that may use the network. Inference never
        // consults the store or filesystem.
        let files = store.selected(self.profile)?;
        if files.manifest.info.dimension != EMBEDDING_DIMENSION
            || files.manifest.info.max_input_tokens != MAX_INPUT_TOKENS
        {
            return Err(EmbeddingError::UnsupportedModel(format!(
                "expected {EMBEDDING_DIMENSION}-dimensional output and a {MAX_INPUT_TOKENS}-token limit, found {} dimensions and a {}-token limit",
                files.manifest.info.dimension, files.manifest.info.max_input_tokens
            )));
        }
        let mut tokenizer = Tokenizer::from_file(&files.tokenizer_path)
            .map_err(|error| EmbeddingError::Tokenizer(error.to_string()))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: files.manifest.info.max_input_tokens,
                strategy: TruncationStrategy::LongestFirst,
                ..TruncationParams::default()
            }))
            .map_err(|error| EmbeddingError::Tokenizer(error.to_string()))?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..PaddingParams::default()
        }));

        let session = Session::builder()
            .map_err(onnx_error)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(onnx_error)?
            .with_intra_threads(self.intra_threads)
            .map_err(onnx_error)?
            .with_inter_threads(self.inter_threads)
            .map_err(onnx_error)?
            .with_parallel_execution(false)
            .map_err(onnx_error)?
            .commit_from_file(&files.model_path)
            .map_err(onnx_error)?;

        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|input| input.name().to_owned())
            .collect();
        if !input_names.iter().any(|name| name == "input_ids")
            || !input_names.iter().any(|name| name == "attention_mask")
        {
            return Err(EmbeddingError::UnsupportedModel(format!(
                "expected input_ids and attention_mask, found {input_names:?}"
            )));
        }
        let output_name = session
            .outputs()
            .iter()
            .find(|output| matches!(output.name(), "sentence_embedding" | "embedding"))
            .or_else(|| session.outputs().first())
            .map(|output| output.name().to_owned())
            .ok_or_else(|| EmbeddingError::UnsupportedModel("model has no output".into()))?;

        let embedder = OnnxTextEmbedder {
            model_info: files.manifest.info,
            tokenizer,
            session: Mutex::new(session),
            input_names,
            output_name,
        };
        if self.warmup {
            embedder.embed("RetrievalKit warmup")?;
        }
        Ok(embedder)
    }
}

static RUNTIME_LIBRARY_PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

fn initialize_runtime(configured_path: Option<&std::path::Path>) -> Result<()> {
    let path = configured_path
        .map(std::path::Path::to_path_buf)
        .or_else(|| std::env::var_os(crate::ONNX_RUNTIME_LIBRARY_ENV).map(PathBuf::from))
        .ok_or_else(|| {
            EmbeddingError::Onnx(format!(
                "official ONNX Runtime {} library path is required; set {} or call runtime_library_path(...)",
                crate::ONNX_RUNTIME_VERSION,
                crate::ONNX_RUNTIME_LIBRARY_ENV
            ))
        })?;
    let mut loaded_path = RUNTIME_LIBRARY_PATH
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| EmbeddingError::SessionPoisoned)?;
    if let Some(existing) = loaded_path.as_ref() {
        if existing != &path {
            return Err(EmbeddingError::Onnx(format!(
                "ONNX Runtime is already initialized from '{}', refusing different path '{}'",
                existing.display(),
                path.display()
            )));
        }
        return Ok(());
    }
    ort::init_from(&path).map_err(onnx_error)?.commit();
    *loaded_path = Some(path);
    Ok(())
}

/// Local ONNX Runtime text embedder.
#[derive(Debug)]
pub struct OnnxTextEmbedder {
    model_info: EmbeddingModelInfo,
    tokenizer: Tokenizer,
    session: Mutex<Session>,
    input_names: Vec<String>,
    output_name: String,
}

impl OnnxTextEmbedder {
    pub fn builder() -> OnnxTextEmbedderBuilder {
        OnnxTextEmbedderBuilder::default()
    }

    pub fn model_info(&self) -> &EmbeddingModelInfo {
        &self.model_info
    }

    fn run(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Err(EmbeddingError::EmptyBatch);
        }
        if texts.iter().any(|text| text.trim().is_empty()) {
            return Err(EmbeddingError::EmptyInput);
        }

        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|error| EmbeddingError::Tokenizer(error.to_string()))?;
        let batch = encodings.len();
        let sequence = encodings
            .first()
            .map(|encoding| encoding.len())
            .ok_or(EmbeddingError::EmptyBatch)?;
        if sequence > self.model_info.max_input_tokens {
            return Err(EmbeddingError::Tokenizer(format!(
                "tokenized sequence length {sequence} exceeds maximum {}",
                self.model_info.max_input_tokens
            )));
        }

        let mut input_ids = Vec::with_capacity(batch * sequence);
        let mut attention_mask = Vec::with_capacity(batch * sequence);
        let mut token_type_ids = Vec::with_capacity(batch * sequence);
        for encoding in &encodings {
            if encoding.len() != sequence {
                return Err(EmbeddingError::Tokenizer(
                    "batch-longest padding produced inconsistent lengths".into(),
                ));
            }
            input_ids.extend(encoding.get_ids().iter().map(|value| i64::from(*value)));
            attention_mask.extend(
                encoding
                    .get_attention_mask()
                    .iter()
                    .map(|value| i64::from(*value)),
            );
            token_type_ids.extend(
                encoding
                    .get_type_ids()
                    .iter()
                    .map(|value| i64::from(*value)),
            );
        }

        let shape = [batch, sequence];
        let input_ids = Tensor::from_array((shape, input_ids)).map_err(onnx_error)?;
        let attention_mask = Tensor::from_array((shape, attention_mask)).map_err(onnx_error)?;
        let token_type_ids = Tensor::from_array((shape, token_type_ids)).map_err(onnx_error)?;
        let mut inputs = ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
        ];
        if self.input_names.iter().any(|name| name == "token_type_ids") {
            inputs.push(("token_type_ids".into(), token_type_ids.into_dyn().into()));
        }

        let mut session = self
            .session
            .lock()
            .map_err(|_| EmbeddingError::SessionPoisoned)?;
        let outputs = session.run(inputs).map_err(onnx_error)?;
        let output = outputs.get(&self.output_name).ok_or_else(|| {
            EmbeddingError::UnsupportedModel(format!(
                "model did not return configured output '{}'",
                self.output_name
            ))
        })?;
        let (output_shape, values) = output.try_extract_tensor::<f32>().map_err(onnx_error)?;
        pool_and_normalize(
            output_shape,
            values,
            &attention_mask_values(&encodings),
            batch,
            sequence,
            self.model_info.dimension,
        )
    }
}

impl TextEmbedder for OnnxTextEmbedder {
    fn model_info(&self) -> &EmbeddingModelInfo {
        &self.model_info
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.run(&[text])?
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::InvalidOutput("model returned no embeddings".into()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.run(texts)
    }
}

fn attention_mask_values(encodings: &[tokenizers::Encoding]) -> Vec<i64> {
    encodings
        .iter()
        .flat_map(|encoding| {
            encoding
                .get_attention_mask()
                .iter()
                .map(|value| i64::from(*value))
        })
        .collect()
}

fn pool_and_normalize(
    shape: &[i64],
    values: &[f32],
    attention_mask: &[i64],
    batch: usize,
    sequence: usize,
    dimension: usize,
) -> Result<Vec<Vec<f32>>> {
    let mut embeddings = match shape {
        [output_batch, output_dimension]
            if *output_batch == batch as i64 && *output_dimension == dimension as i64 =>
        {
            values
                .chunks_exact(dimension)
                .map(<[f32]>::to_vec)
                .collect()
        }
        [output_batch, output_sequence, output_dimension]
            if *output_batch == batch as i64
                && *output_sequence == sequence as i64
                && *output_dimension == dimension as i64 =>
        {
            mean_pool(values, attention_mask, batch, sequence, dimension)?
        }
        _ => {
            return Err(EmbeddingError::UnsupportedModel(format!(
                "expected [{batch}, {dimension}] or [{batch}, {sequence}, {dimension}] output, found {shape:?}"
            )));
        }
    };

    for embedding in &mut embeddings {
        normalize_and_validate(embedding, dimension)?;
    }
    Ok(embeddings)
}

fn mean_pool(
    values: &[f32],
    attention_mask: &[i64],
    batch: usize,
    sequence: usize,
    dimension: usize,
) -> Result<Vec<Vec<f32>>> {
    if values.len() != batch * sequence * dimension || attention_mask.len() != batch * sequence {
        return Err(EmbeddingError::InvalidOutput(
            "model output and attention-mask sizes are inconsistent".into(),
        ));
    }
    let mut embeddings = vec![vec![0.0; dimension]; batch];
    for batch_index in 0..batch {
        let mut token_count = 0.0_f32;
        for token_index in 0..sequence {
            if attention_mask[batch_index * sequence + token_index] == 0 {
                continue;
            }
            token_count += 1.0;
            let offset = (batch_index * sequence + token_index) * dimension;
            for (sum, value) in embeddings[batch_index]
                .iter_mut()
                .zip(&values[offset..offset + dimension])
            {
                *sum += *value;
            }
        }
        if token_count == 0.0 {
            return Err(EmbeddingError::InvalidOutput(
                "attention mask contains no active tokens".into(),
            ));
        }
        for value in &mut embeddings[batch_index] {
            *value /= token_count;
        }
    }
    Ok(embeddings)
}

fn normalize_and_validate(embedding: &mut [f32], dimension: usize) -> Result<()> {
    if embedding.len() != dimension {
        return Err(EmbeddingError::InvalidOutput(format!(
            "expected {dimension} values, found {}",
            embedding.len()
        )));
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err(EmbeddingError::InvalidOutput(
            "embedding contains a non-finite value".into(),
        ));
    }
    let squared_norm: f32 = embedding.iter().map(|value| value * value).sum();
    if !squared_norm.is_finite() || squared_norm <= f32::EPSILON {
        return Err(EmbeddingError::InvalidOutput(
            "embedding has zero or invalid L2 norm".into(),
        ));
    }
    let inverse_norm = squared_norm.sqrt().recip();
    for value in embedding {
        *value *= inverse_norm;
    }
    Ok(())
}

fn onnx_error<R>(error: ort::Error<R>) -> EmbeddingError {
    EmbeddingError::Onnx(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_uses_the_canonical_fp32_profile_by_default() {
        assert_eq!(
            OnnxTextEmbedderBuilder::default().profile,
            EmbeddingProfile::Fp32
        );
    }

    #[test]
    fn mean_pool_ignores_padding_and_normalizes() {
        let values = [
            1.0, 0.0, 0.0, // active
            1.0, 0.0, 0.0, // active
            99.0, 99.0, 99.0, // padding
        ];
        let embeddings = pool_and_normalize(&[1, 3, 3], &values, &[1, 1, 0], 1, 3, 3).unwrap();
        assert_eq!(embeddings[0], vec![1.0, 0.0, 0.0]);
    }

    #[test]
    fn rejects_non_finite_and_zero_outputs() {
        assert!(normalize_and_validate(&mut [f32::NAN, 1.0], 2).is_err());
        assert!(normalize_and_validate(&mut [0.0, 0.0], 2).is_err());
    }

    #[test]
    fn pooled_batch_output_is_normalized() {
        let embeddings =
            pool_and_normalize(&[2, 2], &[3.0, 4.0, 0.0, 2.0], &[1, 1], 2, 1, 2).unwrap();
        assert!((embeddings[0][0] - 0.6).abs() < 1e-6);
        assert!((embeddings[0][1] - 0.8).abs() < 1e-6);
        assert_eq!(embeddings[1], vec![0.0, 1.0]);
    }
}
