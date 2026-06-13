use coreml_native::{BorrowedTensor, ComputeUnits, Model};
use std::path::{Path, PathBuf};
use tokenizers::{
    EncodeInput, PaddingDirection, PaddingParams, PaddingStrategy, Tokenizer, TruncationParams,
};

const DEFAULT_SEQUENCE_LENGTH: usize = 256;
const DEFAULT_OUTPUT_NAME: &str = "embedding";

#[derive(Debug, Clone)]
pub struct CoreMlEmbeddingConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub sequence_length: usize,
    pub output_name: String,
    pub compute_units: ComputeUnits,
}

impl CoreMlEmbeddingConfig {
    pub fn new(model_path: impl Into<PathBuf>, tokenizer_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            tokenizer_path: tokenizer_path.into(),
            sequence_length: DEFAULT_SEQUENCE_LENGTH,
            output_name: DEFAULT_OUTPUT_NAME.to_string(),
            compute_units: ComputeUnits::All,
        }
    }

    pub fn with_sequence_length(mut self, sequence_length: usize) -> Self {
        self.sequence_length = sequence_length;
        self
    }

    pub fn with_output_name(mut self, output_name: impl Into<String>) -> Self {
        self.output_name = output_name.into();
        self
    }

    pub fn with_compute_units(mut self, compute_units: ComputeUnits) -> Self {
        self.compute_units = compute_units;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("sequence length must be greater than zero")]
    EmptySequenceLength,

    #[error("failed to load tokenizer at {path}: {message}")]
    TokenizerLoad { path: PathBuf, message: String },

    #[error("failed to configure tokenizer: {0}")]
    TokenizerConfig(String),

    #[error("failed to tokenize input: {0}")]
    Tokenization(String),

    #[error("tokenizer produced invalid lengths: input_ids={input_ids}, attention_mask={attention_mask}, token_type_ids={token_type_ids}, expected={expected}")]
    InvalidTokenLengths {
        input_ids: usize,
        attention_mask: usize,
        token_type_ids: usize,
        expected: usize,
    },

    #[error("embedding output buffer is too small: got {actual}, expected at least {expected}")]
    OutputBufferTooSmall { actual: usize, expected: usize },

    #[error("Core ML error: {0}")]
    CoreMl(#[from] coreml_native::Error),
}

pub type Result<T> = std::result::Result<T, EmbeddingError>;

pub struct CoreMlTextEmbedder {
    tokenizer: Tokenizer,
    model: Model,
    sequence_length: usize,
    output_name: String,
}

impl CoreMlTextEmbedder {
    pub fn load(config: CoreMlEmbeddingConfig) -> Result<Self> {
        if config.sequence_length == 0 {
            return Err(EmbeddingError::EmptySequenceLength);
        }

        let tokenizer = load_tokenizer(&config.tokenizer_path, config.sequence_length)?;
        let model = Model::load(&config.model_path, config.compute_units)?;

        Ok(Self {
            tokenizer,
            model,
            sequence_length: config.sequence_length,
            output_name: config.output_name,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let tokenized = self.tokenize(text)?;
        let prediction = self.predict(&tokenized)?;
        let (embedding, _) = prediction.get_f32(&self.output_name)?;
        Ok(embedding)
    }

    pub fn embed_into(&self, text: &str, output: &mut [f32]) -> Result<usize> {
        let tokenized = self.tokenize(text)?;
        let prediction = self.predict(&tokenized)?;
        let shape = prediction.get_f32_into(&self.output_name, output)?;
        let count = element_count(&shape);
        if output.len() < count {
            return Err(EmbeddingError::OutputBufferTooSmall {
                actual: output.len(),
                expected: count,
            });
        }
        Ok(count)
    }

    pub fn sequence_length(&self) -> usize {
        self.sequence_length
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn model_path(&self) -> &Path {
        self.model.path()
    }

    fn tokenize(&self, text: &str) -> Result<TokenizedInput> {
        let encoded = self
            .tokenizer
            .encode(EncodeInput::Single(text.into()), true)
            .map_err(|error| EmbeddingError::Tokenization(error.to_string()))?;

        let input_ids: Vec<i32> = encoded.get_ids().iter().map(|&id| id as i32).collect();
        let attention_mask: Vec<i32> = encoded
            .get_attention_mask()
            .iter()
            .map(|&value| value as i32)
            .collect();
        let token_type_ids: Vec<i32> = encoded.get_type_ids().iter().map(|&id| id as i32).collect();

        if input_ids.len() != self.sequence_length
            || attention_mask.len() != self.sequence_length
            || token_type_ids.len() != self.sequence_length
        {
            return Err(EmbeddingError::InvalidTokenLengths {
                input_ids: input_ids.len(),
                attention_mask: attention_mask.len(),
                token_type_ids: token_type_ids.len(),
                expected: self.sequence_length,
            });
        }

        Ok(TokenizedInput {
            input_ids,
            attention_mask,
            token_type_ids,
        })
    }

    fn predict(&self, tokenized: &TokenizedInput) -> Result<coreml_native::Prediction> {
        let shape = [1, self.sequence_length];
        let input_ids = BorrowedTensor::from_i32(&tokenized.input_ids, &shape)?;
        let attention_mask = BorrowedTensor::from_i32(&tokenized.attention_mask, &shape)?;
        let token_type_ids = BorrowedTensor::from_i32(&tokenized.token_type_ids, &shape)?;

        self.model
            .predict(&[
                ("input_ids", &input_ids),
                ("attention_mask", &attention_mask),
                ("token_type_ids", &token_type_ids),
            ])
            .map_err(EmbeddingError::CoreMl)
    }
}

fn load_tokenizer(path: &Path, sequence_length: usize) -> Result<Tokenizer> {
    let mut tokenizer =
        Tokenizer::from_file(path).map_err(|error| EmbeddingError::TokenizerLoad {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: sequence_length,
            ..Default::default()
        }))
        .map_err(|error| EmbeddingError::TokenizerConfig(error.to_string()))?;

    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::Fixed(sequence_length),
        direction: PaddingDirection::Right,
        pad_id: 0,
        pad_type_id: 0,
        pad_token: "[PAD]".into(),
        pad_to_multiple_of: None,
    }));

    Ok(tokenizer)
}

fn element_count(shape: &[usize]) -> usize {
    shape.iter().copied().product()
}

struct TokenizedInput {
    input_ids: Vec<i32>,
    attention_mask: Vec<i32>,
    token_type_ids: Vec<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_minilm_defaults() {
        let config = CoreMlEmbeddingConfig::new("model.mlmodelc", "tokenizer.json");

        assert_eq!(config.model_path, PathBuf::from("model.mlmodelc"));
        assert_eq!(config.tokenizer_path, PathBuf::from("tokenizer.json"));
        assert_eq!(config.sequence_length, 256);
        assert_eq!(config.output_name, "embedding");
        assert_eq!(config.compute_units, ComputeUnits::All);
    }

    #[test]
    fn config_overrides_are_chainable() {
        let config = CoreMlEmbeddingConfig::new("model.mlmodelc", "tokenizer.json")
            .with_sequence_length(128)
            .with_output_name("sentence_embedding")
            .with_compute_units(ComputeUnits::CpuOnly);

        assert_eq!(config.sequence_length, 128);
        assert_eq!(config.output_name, "sentence_embedding");
        assert_eq!(config.compute_units, ComputeUnits::CpuOnly);
    }
}
