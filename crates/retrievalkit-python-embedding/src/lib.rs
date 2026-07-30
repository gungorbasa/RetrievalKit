use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use pyo3::{
    create_exception,
    exceptions::{PyException, PyValueError},
    prelude::*,
};
use retrievalkit_embedding::{
    DownloadPolicy, EmbeddingError as RustEmbeddingError, EmbeddingProfile, ModelStore,
    OnnxTextEmbedder, TextEmbedder, EMBEDDING_DIMENSION, ONNX_RUNTIME_LIBRARY_ENV,
    ONNX_RUNTIME_VERSION,
};
use sha2::{Digest, Sha256};

create_exception!(_native, EmbeddingError, PyException);
create_exception!(_native, EmbeddingInputError, PyValueError);
create_exception!(_native, ModelUnavailableError, EmbeddingError);
create_exception!(_native, ArtifactError, EmbeddingError);
create_exception!(_native, DownloadError, ArtifactError);
create_exception!(_native, EmbeddingRuntimeError, EmbeddingError);

const PACKAGED_RUNTIME_FILENAME: &str = "libonnxruntime.1.24.3.dylib";
const PACKAGED_RUNTIME_SIZE: u64 = 27_724_968;
const PACKAGED_RUNTIME_SHA256: &str =
    "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729";

#[pyclass(name = "ModelInfo", frozen, get_all)]
#[derive(Clone)]
struct PyModelInfo {
    identifier: String,
    revision: String,
    profile: String,
    dimension: usize,
    max_input_tokens: usize,
    produces_normalized_embeddings: bool,
}

#[pymethods]
impl PyModelInfo {
    fn __repr__(&self) -> String {
        format!(
            "ModelInfo(identifier={:?}, revision={:?}, profile='fp32', dimension={}, \
             max_input_tokens={}, produces_normalized_embeddings={})",
            self.identifier,
            self.revision,
            self.dimension,
            self.max_input_tokens,
            self.produces_normalized_embeddings
        )
    }
}

#[pyclass(name = "OnnxEmbedder")]
struct PyOnnxEmbedder {
    inner: Arc<OnnxTextEmbedder>,
}

enum RuntimeLibrary {
    ApplicationManaged(Option<PathBuf>),
    PackagedCandidate(PathBuf),
}

#[pymethods]
impl PyOnnxEmbedder {
    #[new]
    #[pyo3(signature = (*, local_only = false, cache_directory = None, runtime_library_path = None))]
    fn new(
        py: Python<'_>,
        local_only: bool,
        cache_directory: Option<PathBuf>,
        runtime_library_path: Option<PathBuf>,
    ) -> PyResult<Self> {
        Self::load(py, local_only, cache_directory, runtime_library_path)
    }

    #[staticmethod]
    #[pyo3(signature = (*, local_only = false, cache_directory = None, runtime_library_path = None))]
    fn load(
        py: Python<'_>,
        local_only: bool,
        cache_directory: Option<PathBuf>,
        runtime_library_path: Option<PathBuf>,
    ) -> PyResult<Self> {
        let runtime_library =
            resolve_runtime_library(py, runtime_library_path).map_err(py_error)?;
        let policy = download_policy(local_only);
        let inner = py.detach(move || {
            let runtime_library_path = match runtime_library {
                RuntimeLibrary::ApplicationManaged(path) => path,
                RuntimeLibrary::PackagedCandidate(path) if path.exists() => {
                    validate_packaged_runtime(&path).map_err(py_error)?;
                    Some(path)
                }
                RuntimeLibrary::PackagedCandidate(_) => None,
            };
            let mut builder = OnnxTextEmbedder::builder().download_policy(policy);
            if let Some(path) = cache_directory {
                builder = builder.cache_dir(path);
            }
            if let Some(path) = runtime_library_path {
                builder = builder.runtime_library_path(path);
            }
            builder.build().map(Arc::new).map_err(py_error)
        })?;
        Ok(Self { inner })
    }

    #[staticmethod]
    #[pyo3(signature = (*, cache_directory = None, local_only = false))]
    fn prefetch(
        py: Python<'_>,
        cache_directory: Option<PathBuf>,
        local_only: bool,
    ) -> PyResult<()> {
        py.detach(move || {
            let policy = download_policy(local_only);
            let store = match cache_directory {
                Some(path) => ModelStore::with_cache_dir(path, policy),
                None => ModelStore::new(policy),
            }
            .map_err(py_error)?;
            store
                .selected(EmbeddingProfile::Fp32)
                .map(|_| ())
                .map_err(py_error)
        })
    }

    #[getter]
    fn model_info(&self) -> PyModelInfo {
        let info = self.inner.model_info();
        PyModelInfo {
            identifier: info.identifier.clone(),
            revision: info.revision.clone(),
            profile: "fp32".to_owned(),
            dimension: info.dimension,
            max_input_tokens: info.max_input_tokens,
            produces_normalized_embeddings: true,
        }
    }

    fn embed(&self, py: Python<'_>, text: String) -> PyResult<Vec<f32>> {
        let inner = Arc::clone(&self.inner);
        py.detach(move || {
            let embedding = inner.embed(&text).map_err(py_error)?;
            validate_embedding(&embedding)?;
            Ok(embedding)
        })
    }

    fn embed_batch(&self, py: Python<'_>, texts: Vec<String>) -> PyResult<Vec<Vec<f32>>> {
        let inner = Arc::clone(&self.inner);
        py.detach(move || {
            let inputs: Vec<&str> = texts.iter().map(String::as_str).collect();
            let embeddings = inner.embed_batch(&inputs).map_err(py_error)?;
            for embedding in &embeddings {
                validate_embedding(embedding)?;
            }
            Ok(embeddings)
        })
    }
}

fn download_policy(local_only: bool) -> DownloadPolicy {
    if local_only {
        DownloadPolicy::LocalOnly
    } else {
        DownloadPolicy::DownloadIfMissing
    }
}

fn validate_embedding(embedding: &[f32]) -> PyResult<()> {
    if embedding.len() != EMBEDDING_DIMENSION {
        return Err(EmbeddingRuntimeError::new_err(format!(
            "expected {EMBEDDING_DIMENSION} values, found {}",
            embedding.len()
        )));
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err(EmbeddingRuntimeError::new_err(
            "embedding contains a non-finite value",
        ));
    }
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !norm.is_finite() || (norm - 1.0).abs() > 1.0e-4 {
        return Err(EmbeddingRuntimeError::new_err(format!(
            "embedding L2 norm must be 1.0, found {norm}"
        )));
    }
    Ok(())
}

fn resolve_runtime_library(
    py: Python<'_>,
    explicit: Option<PathBuf>,
) -> Result<RuntimeLibrary, RustEmbeddingError> {
    if explicit.is_some() || std::env::var_os(ONNX_RUNTIME_LIBRARY_ENV).is_some() {
        return Ok(RuntimeLibrary::ApplicationManaged(explicit));
    }
    let module = PyModule::import(py, "retrievalkit_embedding._native")
        .map_err(|error| RustEmbeddingError::Onnx(error.to_string()))?;
    let module_file: PathBuf = module
        .getattr("__file__")
        .and_then(|value| value.extract())
        .map_err(|error| RustEmbeddingError::Onnx(error.to_string()))?;
    let package_directory = module_file.parent().ok_or_else(|| {
        RustEmbeddingError::Onnx("Python extension module has no package directory".into())
    })?;
    let packaged = package_directory
        .join("runtime")
        .join(PACKAGED_RUNTIME_FILENAME);
    Ok(RuntimeLibrary::PackagedCandidate(packaged))
}

fn validate_packaged_runtime(path: &Path) -> Result<(), RustEmbeddingError> {
    let metadata = std::fs::metadata(path).map_err(|source| RustEmbeddingError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() || metadata.len() != PACKAGED_RUNTIME_SIZE {
        return Err(RustEmbeddingError::Onnx(format!(
            "packaged ONNX Runtime {} failed exact-size verification",
            path.display()
        )));
    }
    let file = File::open(path).map_err(|source| RustEmbeddingError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|source| RustEmbeddingError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let actual = format!("{:x}", digest.finalize());
    if actual != PACKAGED_RUNTIME_SHA256 {
        return Err(RustEmbeddingError::Onnx(format!(
            "packaged ONNX Runtime {} failed SHA-256 verification",
            path.display()
        )));
    }
    Ok(())
}

#[pyfunction]
fn _verify_package_runtime(py: Python<'_>, path: PathBuf) -> PyResult<()> {
    py.detach(move || validate_packaged_runtime(&path).map_err(py_error))
}

fn py_error(error: RustEmbeddingError) -> PyErr {
    match error {
        RustEmbeddingError::EmptyInput | RustEmbeddingError::EmptyBatch => {
            EmbeddingInputError::new_err(error.to_string())
        }
        RustEmbeddingError::ModelUnavailable(_) => {
            ModelUnavailableError::new_err(error.to_string())
        }
        RustEmbeddingError::Download { .. } => DownloadError::new_err(error.to_string()),
        RustEmbeddingError::CorruptArtifact { .. }
        | RustEmbeddingError::InsecureUrl(_)
        | RustEmbeddingError::InvalidManifest(_)
        | RustEmbeddingError::Io { .. } => ArtifactError::new_err(error.to_string()),
        RustEmbeddingError::Tokenizer(_)
        | RustEmbeddingError::Onnx(_)
        | RustEmbeddingError::UnsupportedModel(_)
        | RustEmbeddingError::InvalidOutput(_)
        | RustEmbeddingError::SessionPoisoned => EmbeddingRuntimeError::new_err(error.to_string()),
    }
}

#[pymodule]
fn _native(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(_verify_package_runtime, module)?)?;
    module.add_class::<PyOnnxEmbedder>()?;
    module.add_class::<PyModelInfo>()?;
    module.add("EmbeddingError", py.get_type::<EmbeddingError>())?;
    module.add("EmbeddingInputError", py.get_type::<EmbeddingInputError>())?;
    module.add(
        "ModelUnavailableError",
        py.get_type::<ModelUnavailableError>(),
    )?;
    module.add("ArtifactError", py.get_type::<ArtifactError>())?;
    module.add("DownloadError", py.get_type::<DownloadError>())?;
    module.add(
        "EmbeddingRuntimeError",
        py.get_type::<EmbeddingRuntimeError>(),
    )?;
    module.add("EMBEDDING_DIMENSION", EMBEDDING_DIMENSION)?;
    module.add("ONNX_RUNTIME_VERSION", ONNX_RUNTIME_VERSION)?;
    module.add(
        "BUILD_MODE",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    )?;
    Ok(())
}
