use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorKitError {
    InvalidDimension { expected: usize, actual: usize },
    InvalidRange { field: String },
    UnsupportedVectorEncoding { encoding: String },
    Persistence { operation: String, path: String },
    InvalidFormat { message: String },
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
            Self::Persistence { operation, path } => {
                write!(f, "persistence {operation} failed for '{path}'")
            }
            Self::InvalidFormat { message } => {
                write!(f, "invalid index format: {message}")
            }
        }
    }
}

impl Error for VectorKitError {}

pub type Result<T> = std::result::Result<T, VectorKitError>;
