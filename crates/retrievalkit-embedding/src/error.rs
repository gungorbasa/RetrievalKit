use std::path::PathBuf;

/// Errors produced while locating a model or embedding text.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("input text cannot be empty")]
    EmptyInput,

    #[error("embedding batch cannot be empty")]
    EmptyBatch,

    #[error("model artifact is unavailable in local-only mode: {0}")]
    ModelUnavailable(PathBuf),

    #[error(
        "model artifact is corrupt: {path} (expected {expected_size} bytes and SHA-256 {expected_sha256})"
    )]
    CorruptArtifact {
        path: PathBuf,
        expected_size: u64,
        expected_sha256: String,
    },

    #[error("model artifact URL must use HTTPS: {0}")]
    InsecureUrl(String),

    #[error("invalid model manifest: {0}")]
    InvalidManifest(String),

    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to download {url}: {message}")]
    Download { url: String, message: String },

    #[error("tokenizer error: {0}")]
    Tokenizer(String),

    #[error("ONNX Runtime error: {0}")]
    Onnx(String),

    #[error("unsupported model interface: {0}")]
    UnsupportedModel(String),

    #[error("invalid embedding output: {0}")]
    InvalidOutput(String),

    #[error("embedding session lock is poisoned")]
    SessionPoisoned,
}

pub type Result<T> = std::result::Result<T, EmbeddingError>;

pub(crate) fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> EmbeddingError {
    EmbeddingError::Io {
        path: path.into(),
        source,
    }
}
