use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use napi::{
    bindgen_prelude::{AsyncTask, Env, Float32Array, Task},
    Error, Result, Status,
};
use napi_derive::napi;
use retrievalkit_embedding::{
    DownloadPolicy, EmbeddingError, EmbeddingModelInfo, EmbeddingProfile, ModelStore,
    OnnxTextEmbedder, TextEmbedder, EMBEDDING_DIMENSION, MAX_INPUT_TOKENS, ONNX_RUNTIME_VERSION,
};
use sha2::{Digest, Sha256};

const BUNDLED_RUNTIME_FILENAME: &str = "libonnxruntime.1.24.3.dylib";
const BUNDLED_RUNTIME_SIZE: u64 = 27_724_968;
const BUNDLED_RUNTIME_SHA256: &str =
    "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729";

#[napi(object)]
pub struct NativeLoadOptions {
    pub cache_directory: Option<String>,
    pub local_only: Option<bool>,
    pub runtime_library_path: Option<String>,
    pub verify_package_runtime: Option<bool>,
}

#[napi(object)]
pub struct NativePrefetchOptions {
    pub cache_directory: Option<String>,
    pub local_only: Option<bool>,
}

#[napi(object)]
pub struct NativeModelInfo {
    pub identifier: String,
    pub dimension: u32,
    pub max_input_tokens: u32,
    pub normalized: bool,
    pub precision: String,
    pub source_revision: String,
    pub runtime: String,
    pub runtime_version: String,
}

impl From<&EmbeddingModelInfo> for NativeModelInfo {
    fn from(info: &EmbeddingModelInfo) -> Self {
        Self {
            identifier: info.identifier.clone(),
            dimension: info.dimension as u32,
            max_input_tokens: info.max_input_tokens as u32,
            normalized: info.produces_normalized_embeddings,
            precision: "fp32".into(),
            source_revision: info.revision.clone(),
            runtime: "onnxruntime".into(),
            runtime_version: ONNX_RUNTIME_VERSION.into(),
        }
    }
}

struct EmbedderState {
    embedder: Option<OnnxTextEmbedder>,
}

struct EmbedderShared {
    state: Mutex<EmbedderState>,
    closed: AtomicBool,
}

impl EmbedderShared {
    fn require_open(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(native_error(
                "RK_EMBEDDING_CLOSED",
                "the ONNX embedder is closed",
            ))
        } else {
            Ok(())
        }
    }
}

#[napi]
pub struct NativeOnnxEmbedder {
    shared: Arc<EmbedderShared>,
}

impl Default for NativeOnnxEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl NativeOnnxEmbedder {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(EmbedderShared {
                state: Mutex::new(EmbedderState { embedder: None }),
                closed: AtomicBool::new(false),
            }),
        }
    }

    #[napi]
    pub fn initialize(&self, options: NativeLoadOptions) -> Result<AsyncTask<InitializeTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(InitializeTask {
            shared: Arc::clone(&self.shared),
            options,
        }))
    }

    #[napi]
    pub fn embed(&self, text: String) -> Result<AsyncTask<EmbedTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(EmbedTask {
            shared: Arc::clone(&self.shared),
            text,
        }))
    }

    #[napi(js_name = "embedBatch")]
    pub fn embed_batch(&self, texts: Vec<String>) -> Result<AsyncTask<EmbedBatchTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(EmbedBatchTask {
            shared: Arc::clone(&self.shared),
            texts,
        }))
    }

    #[napi]
    pub fn model_info(&self) -> Result<NativeModelInfo> {
        self.shared.require_open()?;
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| state_poisoned_error())?;
        let embedder = state.embedder.as_ref().ok_or_else(not_loaded_error)?;
        Ok(embedder.model_info().into())
    }

    #[napi]
    pub fn close(&self) -> AsyncTask<CloseTask> {
        self.shared.closed.store(true, Ordering::Release);
        AsyncTask::new(CloseTask {
            shared: Arc::clone(&self.shared),
        })
    }

    #[napi(getter)]
    pub fn closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }
}

#[napi]
pub fn prefetch_model(options: NativePrefetchOptions) -> AsyncTask<PrefetchTask> {
    AsyncTask::new(PrefetchTask { options })
}

/// Private package-verification hook. This is not exported by the public
/// TypeScript entrypoint.
#[napi(js_name = "_verifyPackageRuntime")]
pub fn verify_package_runtime_for_testing(path: String) -> AsyncTask<VerifyRuntimeTask> {
    AsyncTask::new(VerifyRuntimeTask {
        path: PathBuf::from(path),
    })
}

pub struct InitializeTask {
    shared: Arc<EmbedderShared>,
    options: NativeLoadOptions,
}

impl Task for InitializeTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        self.shared.require_open()?;
        let runtime_path = self
            .options
            .runtime_library_path
            .as_deref()
            .map(PathBuf::from);
        if self.options.verify_package_runtime.unwrap_or(false) {
            let path = runtime_path.as_deref().ok_or_else(|| {
                native_error(
                    "RK_EMBEDDING_RUNTIME",
                    "package-local runtime verification requires runtimeLibraryPath",
                )
            })?;
            verify_package_runtime(path)?;
        }

        let mut builder = OnnxTextEmbedder::builder()
            .profile(EmbeddingProfile::Fp32)
            .download_policy(download_policy(self.options.local_only));
        if let Some(cache_directory) = &self.options.cache_directory {
            builder = builder.cache_dir(cache_directory);
        }
        if let Some(runtime_path) = runtime_path {
            builder = builder.runtime_library_path(runtime_path);
        }
        let embedder = builder.build().map_err(embedding_error)?;
        validate_model_info(embedder.model_info())?;

        self.shared.require_open()?;
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| state_poisoned_error())?;
        if state.embedder.is_some() {
            return Err(native_error(
                "RK_EMBEDDING_STATE",
                "the ONNX embedder is already initialized",
            ));
        }
        state.embedder = Some(embedder);
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct PrefetchTask {
    options: NativePrefetchOptions,
}

pub struct VerifyRuntimeTask {
    path: PathBuf,
}

impl Task for VerifyRuntimeTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        verify_package_runtime(&self.path)
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

impl Task for PrefetchTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        let policy = download_policy(self.options.local_only);
        let store = match &self.options.cache_directory {
            Some(path) => ModelStore::with_cache_dir(path, policy),
            None => ModelStore::new(policy),
        }
        .map_err(embedding_error)?;
        let files = store
            .selected(EmbeddingProfile::Fp32)
            .map_err(embedding_error)?;
        validate_model_info(&files.manifest.info)
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct EmbedTask {
    shared: Arc<EmbedderShared>,
    text: String,
}

impl Task for EmbedTask {
    type Output = Vec<f32>;
    type JsValue = Float32Array;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| state_poisoned_error())?;
        let embedder = state.embedder.as_ref().ok_or_else(not_loaded_error)?;
        let output = embedder.embed(&self.text).map_err(embedding_error)?;
        validate_output(&output)?;
        Ok(output)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into())
    }
}

pub struct EmbedBatchTask {
    shared: Arc<EmbedderShared>,
    texts: Vec<String>,
}

impl Task for EmbedBatchTask {
    type Output = Vec<Vec<f32>>;
    type JsValue = Vec<Float32Array>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| state_poisoned_error())?;
        let embedder = state.embedder.as_ref().ok_or_else(not_loaded_error)?;
        let references: Vec<&str> = self.texts.iter().map(String::as_str).collect();
        let output = embedder.embed_batch(&references).map_err(embedding_error)?;
        for embedding in &output {
            validate_output(embedding)?;
        }
        Ok(output)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into_iter().map(Float32Array::from).collect())
    }
}

pub struct CloseTask {
    shared: Arc<EmbedderShared>,
}

impl Task for CloseTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| state_poisoned_error())?;
        state.embedder = None;
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

fn download_policy(local_only: Option<bool>) -> DownloadPolicy {
    if local_only.unwrap_or(false) {
        DownloadPolicy::LocalOnly
    } else {
        DownloadPolicy::DownloadIfMissing
    }
}

fn validate_model_info(info: &EmbeddingModelInfo) -> Result<()> {
    if info.profile != EmbeddingProfile::Fp32
        || info.dimension != EMBEDDING_DIMENSION
        || info.max_input_tokens != MAX_INPUT_TOKENS
        || !info.produces_normalized_embeddings
    {
        return Err(native_error(
            "RK_EMBEDDING_MODEL",
            &format!(
                "expected the canonical FP32 {EMBEDDING_DIMENSION}-dimension, {MAX_INPUT_TOKENS}-token normalized model"
            ),
        ));
    }
    Ok(())
}

fn validate_output(output: &[f32]) -> Result<()> {
    if output.len() != EMBEDDING_DIMENSION {
        return Err(native_error(
            "RK_EMBEDDING_OUTPUT",
            &format!(
                "expected {EMBEDDING_DIMENSION} output values, found {}",
                output.len()
            ),
        ));
    }
    if output.iter().any(|value| !value.is_finite()) {
        return Err(native_error(
            "RK_EMBEDDING_OUTPUT",
            "embedding contains a non-finite value",
        ));
    }
    let norm = output.iter().map(|value| value * value).sum::<f32>().sqrt();
    if (norm - 1.0).abs() > 1e-4 {
        return Err(native_error(
            "RK_EMBEDDING_OUTPUT",
            &format!("embedding L2 norm must be 1.0, found {norm}"),
        ));
    }
    Ok(())
}

fn verify_package_runtime(path: &Path) -> Result<()> {
    if path.file_name().and_then(|name| name.to_str()) != Some(BUNDLED_RUNTIME_FILENAME) {
        return Err(native_error(
            "RK_EMBEDDING_RUNTIME",
            &format!("package-local runtime must be named {BUNDLED_RUNTIME_FILENAME}"),
        ));
    }
    let metadata = path.metadata().map_err(|error| {
        native_error(
            "RK_EMBEDDING_RUNTIME",
            &format!(
                "cannot inspect package-local runtime '{}': {error}",
                path.display()
            ),
        )
    })?;
    if metadata.len() != BUNDLED_RUNTIME_SIZE {
        return Err(native_error(
            "RK_EMBEDDING_RUNTIME",
            &format!(
                "package-local runtime '{}' has size {}, expected {BUNDLED_RUNTIME_SIZE}",
                path.display(),
                metadata.len()
            ),
        ));
    }
    let mut file = File::open(path).map_err(|error| {
        native_error(
            "RK_EMBEDDING_RUNTIME",
            &format!(
                "cannot open package-local runtime '{}': {error}",
                path.display()
            ),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            native_error(
                "RK_EMBEDDING_RUNTIME",
                &format!(
                    "cannot verify package-local runtime '{}': {error}",
                    path.display()
                ),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != BUNDLED_RUNTIME_SHA256 {
        return Err(native_error(
            "RK_EMBEDDING_RUNTIME",
            &format!(
                "package-local runtime '{}' failed SHA-256 verification",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn embedding_error(error: EmbeddingError) -> Error {
    let code = match error {
        EmbeddingError::EmptyInput | EmbeddingError::EmptyBatch => "RK_EMBEDDING_INPUT",
        EmbeddingError::ModelUnavailable(_) => "RK_EMBEDDING_UNAVAILABLE",
        EmbeddingError::CorruptArtifact { .. }
        | EmbeddingError::InsecureUrl(_)
        | EmbeddingError::InvalidManifest(_) => "RK_EMBEDDING_ARTIFACT",
        EmbeddingError::Io { .. } | EmbeddingError::Download { .. } => "RK_EMBEDDING_IO",
        EmbeddingError::Tokenizer(_) => "RK_EMBEDDING_TOKENIZER",
        EmbeddingError::Onnx(_) => "RK_EMBEDDING_RUNTIME",
        EmbeddingError::UnsupportedModel(_) => "RK_EMBEDDING_MODEL",
        EmbeddingError::InvalidOutput(_) => "RK_EMBEDDING_OUTPUT",
        EmbeddingError::SessionPoisoned => "RK_EMBEDDING_STATE",
    };
    native_error(code, &error.to_string())
}

fn native_error(code: &str, message: &str) -> Error {
    Error::new(Status::GenericFailure, format!("{code}: {message}"))
}

fn not_loaded_error() -> Error {
    native_error("RK_EMBEDDING_STATE", "the ONNX embedder is not loaded")
}

fn state_poisoned_error() -> Error {
    native_error(
        "RK_EMBEDDING_STATE",
        "the ONNX embedder lock was poisoned by a previous native failure",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_contract_accepts_a_unit_vector() {
        let mut output = vec![0.0; EMBEDDING_DIMENSION];
        output[0] = 1.0;
        validate_output(&output).unwrap();
    }

    #[test]
    fn output_contract_rejects_invalid_values() {
        assert!(validate_output(&[]).is_err());
        let mut output = vec![0.0; EMBEDDING_DIMENSION];
        output[0] = f32::NAN;
        assert!(validate_output(&output).is_err());
        output[0] = 2.0;
        assert!(validate_output(&output).is_err());
    }
}
