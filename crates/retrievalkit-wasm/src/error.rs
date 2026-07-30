use std::fmt::{Display, Formatter};

use wasm_bindgen::JsError;

pub(crate) type Result<T> = std::result::Result<T, BoundaryError>;

#[derive(Debug)]
pub(crate) struct BoundaryError {
    code: &'static str,
    message: String,
}

impl BoundaryError {
    pub fn invalid(path: &str, message: impl Into<String>) -> Self {
        Self {
            code: "RK_INVALID_BOUNDARY",
            message: format!("{path}: {}", message.into()),
        }
    }

    pub fn state(product: &str, expected: &str) -> Self {
        Self {
            code: "RK_INVALID_STATE",
            message: format!("{product} is not {expected}"),
        }
    }

    pub fn core(error: retrievalkit_core::RetrievalKitError) -> Self {
        Self {
            code: "RK_CORE",
            message: error.to_string(),
        }
    }

    pub fn graph(error: retrievalkit_graph::GraphError) -> Self {
        Self {
            code: "RK_GRAPH",
            message: error.to_string(),
        }
    }

    pub fn serde(error: serde_wasm_bindgen::Error) -> Self {
        Self {
            code: "RK_INVALID_BOUNDARY",
            message: format!("structured JavaScript value is invalid: {error}"),
        }
    }
}

impl Display for BoundaryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl From<BoundaryError> for JsError {
    fn from(error: BoundaryError) -> Self {
        JsError::new(&error.to_string())
    }
}
