use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalKitError {
    InvalidIdentity {
        kind: &'static str,
        value: String,
        message: String,
    },
    InvalidRecordValue {
        path: String,
        message: String,
    },
    InvalidCandidateScope {
        chunk_id: u64,
        message: String,
    },
    StaleGeneration {
        expected_corpus: String,
        expected_generation: u64,
        actual_corpus: String,
        actual_generation: u64,
    },
    InvalidDimension {
        expected: usize,
        actual: usize,
    },
    MissingEmbedding {
        message: String,
    },
    InvalidRange {
        field: String,
    },
    UnsupportedVectorEncoding {
        encoding: String,
    },
    RetrievalCapabilityUnavailable {
        capability: &'static str,
    },
    Persistence {
        operation: String,
        path: String,
        cause: String,
    },
    InvalidFormat {
        message: String,
    },
    CorruptIndex {
        path: String,
        message: String,
    },
}

impl Display for RetrievalKitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentity {
                kind,
                value,
                message,
            } => write!(f, "invalid {kind} '{value}': {message}"),
            Self::InvalidRecordValue { path, message } => {
                write!(f, "invalid canonical record value at '{path}': {message}")
            }
            Self::InvalidCandidateScope { chunk_id, message } => {
                write!(f, "invalid candidate scope chunk ID {chunk_id}: {message}")
            }
            Self::StaleGeneration {
                expected_corpus,
                expected_generation,
                actual_corpus,
                actual_generation,
            } => write!(
                f,
                "stale candidate scope: expected corpus '{expected_corpus}' generation {expected_generation}, got corpus '{actual_corpus}' generation {actual_generation}; rebuild the scope from the active index"
            ),
            Self::InvalidDimension { expected, actual } => {
                write!(
                    f,
                    "invalid vector dimension: expected {expected}, got {actual}; use the same embedding model for indexing and queries"
                )
            }
            Self::MissingEmbedding { message } => {
                write!(f, "missing embedding: {message}")
            }
            Self::InvalidRange { field } => {
                write!(f, "invalid range filter for metadata field '{field}'")
            }
            Self::UnsupportedVectorEncoding { encoding } => {
                write!(f, "unsupported vector encoding '{encoding}'")
            }
            Self::RetrievalCapabilityUnavailable { capability } => write!(
                f,
                "{capability} retrieval state is unavailable; rebuild or reload the database from its canonical corpus"
            ),
            Self::Persistence {
                operation,
                path,
                cause,
            } => {
                write!(
                    f,
                    "persistence {operation} failed for '{path}': {cause}. Check that the index path exists and is readable when loading, or that its parent directory is writable when saving"
                )
            }
            Self::InvalidFormat { message } => {
                write!(f, "invalid index format: {message}")
            }
            Self::CorruptIndex { path, message } => {
                write!(
                    f,
                    "corrupt index file '{path}': {message}. Restore the index from a known-good copy, or rebuild and replace the index directory"
                )
            }
        }
    }
}

impl Error for RetrievalKitError {}

pub type Result<T> = std::result::Result<T, RetrievalKitError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_error_explains_cause_and_recovery() {
        let error = RetrievalKitError::Persistence {
            operation: "create directory".to_owned(),
            path: "/tmp/index".to_owned(),
            cause: "Not a directory".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "persistence create directory failed for '/tmp/index': Not a directory. Check that the index path exists and is readable when loading, or that its parent directory is writable when saving"
        );
    }
}
