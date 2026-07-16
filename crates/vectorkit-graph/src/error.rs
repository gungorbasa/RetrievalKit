use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    InvalidSchema {
        message: String,
    },
    InvalidRecord {
        record_id: String,
        message: String,
    },
    InvalidQuery {
        message: String,
    },
    InvalidSnapshot {
        message: String,
    },
    StaleGeneration {
        message: String,
    },
    IncompatibleVersion {
        message: String,
    },
    GraphUnavailable {
        message: String,
    },
    WriterBusy {
        path: String,
    },
    MissingTarget {
        relationship: String,
        source_record_id: String,
        target_record_id: String,
    },
    QueryLimitExceeded {
        message: String,
    },
    Cancelled,
    TimedOut {
        message: String,
    },
    Core {
        message: String,
    },
}

impl Display for GraphError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSchema { message } => write!(f, "invalid graph schema: {message}"),
            Self::InvalidRecord { record_id, message } => {
                write!(f, "invalid graph record '{record_id}': {message}")
            }
            Self::InvalidQuery { message } => write!(f, "invalid graph query: {message}"),
            Self::InvalidSnapshot { message } => write!(f, "invalid graph snapshot: {message}"),
            Self::StaleGeneration { message } => write!(f, "stale graph generation: {message}"),
            Self::IncompatibleVersion { message } => {
                write!(f, "incompatible graph version: {message}")
            }
            Self::GraphUnavailable { message } => write!(f, "graph unavailable: {message}"),
            Self::WriterBusy { path } => {
                write!(f, "graph database writer is already active for '{path}'")
            }
            Self::MissingTarget {
                relationship,
                source_record_id,
                target_record_id,
            } => write!(
                f,
                "relationship '{relationship}' from record '{source_record_id}' references missing target '{target_record_id}'"
            ),
            Self::QueryLimitExceeded { message } => {
                write!(f, "graph query limit exceeds the engine safety cap: {message}")
            }
            Self::Cancelled => write!(f, "graph query was cancelled"),
            Self::TimedOut { message } => write!(f, "graph operation timed out: {message}"),
            Self::Core { message } => write!(f, "VectorKit core operation failed: {message}"),
        }
    }
}

impl Error for GraphError {}

impl From<vectorkit_core::VectorKitError> for GraphError {
    fn from(error: vectorkit_core::VectorKitError) -> Self {
        match error {
            vectorkit_core::VectorKitError::StaleGeneration { .. } => Self::StaleGeneration {
                message: error.to_string(),
            },
            _ => Self::Core {
                message: error.to_string(),
            },
        }
    }
}

pub type Result<T> = std::result::Result<T, GraphError>;
