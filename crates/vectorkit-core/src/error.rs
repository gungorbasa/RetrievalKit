use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorKitError {
    InvalidDimension {
        expected: usize,
        actual: usize,
    },
    InvalidRange {
        field: String,
    },
    UnsupportedVectorEncoding {
        encoding: String,
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

impl Display for VectorKitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimension { expected, actual } => {
                write!(
                    f,
                    "invalid vector dimension: expected {expected}, got {actual}"
                )
            }
            Self::InvalidRange { field } => {
                write!(f, "invalid range filter for metadata field '{field}'")
            }
            Self::UnsupportedVectorEncoding { encoding } => {
                write!(f, "unsupported vector encoding '{encoding}'")
            }
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

impl Error for VectorKitError {}

pub type Result<T> = std::result::Result<T, VectorKitError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_error_explains_cause_and_recovery() {
        let error = VectorKitError::Persistence {
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
