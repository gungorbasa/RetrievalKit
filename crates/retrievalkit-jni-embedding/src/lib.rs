//! Private typed JNI boundary for the optional Kotlin embedding package.
//!
//! Model acquisition and ONNX Runtime initialization happen only in `load` or
//! `prefetch`. Embedding calls operate on a previously constructed, local
//! [`OnnxTextEmbedder`]. RetrievalKit's retrieval core does not depend on this
//! crate or on ONNX Runtime.

#![deny(unsafe_op_in_unsafe_fn)]

use std::{
    collections::HashMap,
    path::PathBuf,
    ptr,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, LazyLock, Mutex, MutexGuard,
    },
};

use jni::{
    objects::{JClass, JFloatArray, JObject, JObjectArray, JString, JThrowable, JValue},
    sys::{jboolean, jfloatArray, jint, jlong, jobjectArray, jstring, JNI_FALSE},
    JNIEnv,
};
use retrievalkit_embedding::{
    DownloadPolicy, EmbeddingError, EmbeddingModelInfo, EmbeddingProfile, ModelStore,
    OnnxTextEmbedder, TextEmbedder, EMBEDDING_DIMENSION, ONNX_RUNTIME_VERSION,
};

const INVALID_INPUT_EXCEPTION: &str = "ai/retrievalkit/embedding/InvalidEmbeddingInputException";
const MODEL_ACQUISITION_EXCEPTION: &str = "ai/retrievalkit/embedding/ModelAcquisitionException";
const MODEL_INTEGRITY_EXCEPTION: &str = "ai/retrievalkit/embedding/ModelIntegrityException";
const MODEL_LOAD_EXCEPTION: &str = "ai/retrievalkit/embedding/ModelLoadException";
const INFERENCE_EXCEPTION: &str = "ai/retrievalkit/embedding/EmbeddingInferenceException";
const NATIVE_EXCEPTION: &str = "ai/retrievalkit/embedding/NativeLibraryException";
const CLOSED_EXCEPTION: &str = "ai/retrievalkit/embedding/ClosedEmbedderException";

type BoundaryResult<T> = Result<T, BoundaryError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operation {
    Acquisition,
    Load,
    Inference,
}

#[derive(Debug, Eq, PartialEq)]
struct BoundaryError {
    class: &'static str,
    message: String,
}

impl BoundaryError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            class: INVALID_INPUT_EXCEPTION,
            message: message.into(),
        }
    }

    fn closed(handle: jlong) -> Self {
        Self {
            class: CLOSED_EXCEPTION,
            message: format!("native embedder handle {handle} is closed or invalid"),
        }
    }

    fn native(message: impl Into<String>) -> Self {
        Self {
            class: NATIVE_EXCEPTION,
            message: message.into(),
        }
    }

    fn embedding(error: EmbeddingError, operation: Operation) -> Self {
        let class = match error {
            EmbeddingError::EmptyInput | EmbeddingError::EmptyBatch => INVALID_INPUT_EXCEPTION,
            EmbeddingError::ModelUnavailable(_) | EmbeddingError::Download { .. } => {
                MODEL_ACQUISITION_EXCEPTION
            }
            EmbeddingError::CorruptArtifact { .. }
            | EmbeddingError::InsecureUrl(_)
            | EmbeddingError::InvalidManifest(_)
            | EmbeddingError::Io { .. } => MODEL_INTEGRITY_EXCEPTION,
            EmbeddingError::Tokenizer(_)
            | EmbeddingError::Onnx(_)
            | EmbeddingError::UnsupportedModel(_)
            | EmbeddingError::InvalidOutput(_)
            | EmbeddingError::SessionPoisoned => match operation {
                Operation::Inference => INFERENCE_EXCEPTION,
                Operation::Acquisition | Operation::Load => MODEL_LOAD_EXCEPTION,
            },
        };
        Self {
            class,
            message: error.to_string(),
        }
    }
}

impl From<jni::errors::Error> for BoundaryError {
    fn from(error: jni::errors::Error) -> Self {
        Self::native(format!("JNI conversion failed: {error}"))
    }
}

static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);
static EMBEDDERS: LazyLock<Mutex<HashMap<jlong, Arc<OnnxTextEmbedder>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn embedders() -> BoundaryResult<MutexGuard<'static, HashMap<jlong, Arc<OnnxTextEmbedder>>>> {
    EMBEDDERS
        .lock()
        .map_err(|_| BoundaryError::native("native embedder registry is poisoned"))
}

fn insert_embedder(embedder: OnnxTextEmbedder) -> BoundaryResult<jlong> {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    if handle <= 0 {
        return Err(BoundaryError::native(
            "native embedder handle space is exhausted",
        ));
    }
    embedders()?.insert(handle, Arc::new(embedder));
    Ok(handle)
}

fn embedder(handle: jlong) -> BoundaryResult<Arc<OnnxTextEmbedder>> {
    if handle <= 0 {
        return Err(BoundaryError::closed(handle));
    }
    embedders()?
        .get(&handle)
        .cloned()
        .ok_or_else(|| BoundaryError::closed(handle))
}

fn close_embedder(handle: jlong) -> BoundaryResult<()> {
    if handle <= 0 {
        return Err(BoundaryError::closed(handle));
    }
    embedders()?
        .remove(&handle)
        .map(|_| ())
        .ok_or_else(|| BoundaryError::closed(handle))
}

fn with_env<'local, T: Copy>(
    env: &mut JNIEnv<'local>,
    default: T,
    operation: impl FnOnce(&mut JNIEnv<'local>) -> BoundaryResult<T>,
) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(env))) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let _ = throw_boundary_error(env, &error);
            default
        }
        Err(_) => {
            let _ = throw_boundary_error(
                env,
                &BoundaryError::native(
                    "native RetrievalKit embedding operation panicked; close the embedder and report this bug",
                ),
            );
            default
        }
    }
}

fn throw_boundary_error(env: &mut JNIEnv<'_>, error: &BoundaryError) -> jni::errors::Result<()> {
    // Kotlin's public failure types retain an optional cause, so their stable
    // JVM constructor is `(String, Throwable)`. ClosedEmbedderException is the
    // deliberate exception: its public constructor only takes the message.
    if error.class == CLOSED_EXCEPTION {
        return env.throw_new(error.class, &error.message);
    }
    let message = JObject::from(env.new_string(&error.message)?);
    let cause = JObject::null();
    let exception = env.new_object(
        error.class,
        "(Ljava/lang/String;Ljava/lang/Throwable;)V",
        &[JValue::Object(&message), JValue::Object(&cause)],
    )?;
    env.throw(JThrowable::from(exception))
}

fn optional_string(
    env: &mut JNIEnv<'_>,
    value: &JString<'_>,
    field: &str,
) -> BoundaryResult<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    let value: String = env.get_string(value)?.into();
    if value.contains('\0') {
        return Err(BoundaryError::invalid(format!(
            "{field} cannot contain a NUL character"
        )));
    }
    Ok(Some(value))
}

fn required_string(
    env: &mut JNIEnv<'_>,
    value: &JString<'_>,
    field: &str,
) -> BoundaryResult<String> {
    optional_string(env, value, field)?
        .ok_or_else(|| BoundaryError::invalid(format!("{field} cannot be null")))
}

fn download_policy(local_only: jboolean) -> DownloadPolicy {
    if local_only == JNI_FALSE {
        DownloadPolicy::DownloadIfMissing
    } else {
        DownloadPolicy::LocalOnly
    }
}

fn validate_threads(value: jint, field: &str) -> BoundaryResult<usize> {
    if value <= 0 {
        return Err(BoundaryError::invalid(format!(
            "{field} must be greater than zero"
        )));
    }
    usize::try_from(value)
        .map_err(|_| BoundaryError::invalid(format!("{field} is outside the supported range")))
}

fn validate_output(output: &[f32]) -> BoundaryResult<()> {
    if output.len() != EMBEDDING_DIMENSION {
        return Err(BoundaryError {
            class: INFERENCE_EXCEPTION,
            message: format!(
                "expected {EMBEDDING_DIMENSION} embedding values, found {}",
                output.len()
            ),
        });
    }
    if output.iter().any(|value| !value.is_finite()) {
        return Err(BoundaryError {
            class: INFERENCE_EXCEPTION,
            message: "embedding contains a non-finite value".into(),
        });
    }
    let norm = output.iter().map(|value| value * value).sum::<f32>().sqrt();
    if !norm.is_finite() || (norm - 1.0).abs() > 1.0e-4 {
        return Err(BoundaryError {
            class: INFERENCE_EXCEPTION,
            message: format!("embedding L2 norm must be 1.0, found {norm}"),
        });
    }
    Ok(())
}

fn new_float_array<'local>(
    env: &mut JNIEnv<'local>,
    values: &[f32],
) -> BoundaryResult<JFloatArray<'local>> {
    let length = i32::try_from(values.len())
        .map_err(|_| BoundaryError::native("embedding is too large for a Java array"))?;
    let array = env.new_float_array(length)?;
    env.set_float_array_region(&array, 0, values)?;
    Ok(array)
}

fn new_string(env: &mut JNIEnv<'_>, value: &str) -> BoundaryResult<jstring> {
    Ok(env.new_string(value)?.into_raw())
}

fn model_info(handle: jlong) -> BoundaryResult<(Arc<OnnxTextEmbedder>, EmbeddingModelInfo)> {
    let embedder = embedder(handle)?;
    let info = embedder.model_info().clone();
    Ok((embedder, info))
}

/// Constructs the canonical FP32 embedder. This is the only JNI operation
/// besides `prefetch` that may acquire model artifacts over HTTPS.
#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_load(
    mut env: JNIEnv,
    _class: JClass,
    cache_directory: JString,
    local_only: jboolean,
    runtime_library_path: JString,
    intra_threads: jint,
    inter_threads: jint,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let cache_directory = optional_string(env, &cache_directory, "cacheDirectory")?;
        let runtime_library_path =
            optional_string(env, &runtime_library_path, "runtimeLibraryPath")?;
        let intra_threads = validate_threads(intra_threads, "intraThreads")?;
        let inter_threads = validate_threads(inter_threads, "interThreads")?;

        let mut builder = OnnxTextEmbedder::builder()
            .profile(EmbeddingProfile::Fp32)
            .download_policy(download_policy(local_only))
            .intra_threads(intra_threads)
            .inter_threads(inter_threads);
        if let Some(path) = cache_directory {
            builder = builder.cache_dir(PathBuf::from(path));
        }
        if let Some(path) = runtime_library_path {
            if path.trim().is_empty() {
                return Err(BoundaryError::invalid("runtimeLibraryPath cannot be blank"));
            }
            builder = builder.runtime_library_path(PathBuf::from(path));
        }
        let embedder = builder
            .build()
            .map_err(|error| BoundaryError::embedding(error, Operation::Load))?;
        validate_model_info(embedder.model_info())?;
        insert_embedder(embedder)
    })
}

/// Acquires and verifies the canonical FP32 artifact without initializing ONNX
/// Runtime or creating a native embedder handle.
#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_prefetch(
    mut env: JNIEnv,
    _class: JClass,
    cache_directory: JString,
    local_only: jboolean,
) {
    with_env(&mut env, (), |env| {
        let cache_directory = optional_string(env, &cache_directory, "cacheDirectory")?;
        let policy = download_policy(local_only);
        let store = match cache_directory {
            Some(path) => ModelStore::with_cache_dir(PathBuf::from(path), policy),
            None => ModelStore::new(policy),
        }
        .map_err(|error| BoundaryError::embedding(error, Operation::Acquisition))?;
        let files = store
            .selected(EmbeddingProfile::Fp32)
            .map_err(|error| BoundaryError::embedding(error, Operation::Acquisition))?;
        validate_model_info(&files.manifest.info)
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_embed(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    text: JString,
) -> jfloatArray {
    with_env(&mut env, ptr::null_mut(), |env| {
        let text = required_string(env, &text, "text")?;
        if text.trim().is_empty() {
            return Err(BoundaryError::invalid("input text cannot be empty"));
        }
        let output = embedder(handle)?
            .embed(&text)
            .map_err(|error| BoundaryError::embedding(error, Operation::Inference))?;
        validate_output(&output)?;
        Ok(new_float_array(env, &output)?.into_raw())
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_embedBatch(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    texts: JObjectArray,
) -> jobjectArray {
    with_env(&mut env, ptr::null_mut(), |env| {
        if texts.is_null() {
            return Err(BoundaryError::invalid("texts cannot be null"));
        }
        let length = env.get_array_length(&texts)?;
        if length == 0 {
            return Err(BoundaryError::invalid("embedding batch cannot be empty"));
        }
        let mut owned = Vec::with_capacity(length as usize);
        for index in 0..length {
            let value = env.get_object_array_element(&texts, index)?;
            let value = JString::from(value);
            let text = required_string(env, &value, &format!("texts[{index}]"))?;
            if text.trim().is_empty() {
                return Err(BoundaryError::invalid(format!(
                    "texts[{index}] cannot be empty"
                )));
            }
            owned.push(text);
        }
        let references: Vec<&str> = owned.iter().map(String::as_str).collect();
        let outputs = embedder(handle)?
            .embed_batch(&references)
            .map_err(|error| BoundaryError::embedding(error, Operation::Inference))?;
        if outputs.len() != owned.len() {
            return Err(BoundaryError {
                class: INFERENCE_EXCEPTION,
                message: format!(
                    "expected {} embeddings, found {}",
                    owned.len(),
                    outputs.len()
                ),
            });
        }

        let float_array_class = env.find_class("[F")?;
        let result = env.new_object_array(length, float_array_class, JObject::null())?;
        for (index, output) in outputs.iter().enumerate() {
            validate_output(output)?;
            let output = new_float_array(env, output)?;
            env.set_object_array_element(&result, index as i32, &output)?;
        }
        Ok(result.into_raw())
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_modelIdentifier(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    with_env(&mut env, ptr::null_mut(), |env| {
        let (_, info) = model_info(handle)?;
        new_string(env, &info.identifier)
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_modelRevision(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    with_env(&mut env, ptr::null_mut(), |env| {
        let (_, info) = model_info(handle)?;
        new_string(env, &info.revision)
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_modelPrecision(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    with_env(&mut env, ptr::null_mut(), |env| {
        let (_, info) = model_info(handle)?;
        new_string(env, info.profile.as_str())
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_modelDimension(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    with_env(&mut env, 0, |_env| {
        let (_, info) = model_info(handle)?;
        i32::try_from(info.dimension)
            .map_err(|_| BoundaryError::native("model dimension is outside the JNI integer range"))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_modelMaxInputTokens(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    with_env(&mut env, 0, |_env| {
        let (_, info) = model_info(handle)?;
        i32::try_from(info.max_input_tokens).map_err(|_| {
            BoundaryError::native("maximum input tokens is outside the JNI integer range")
        })
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_modelProducesNormalizedEmbeddings(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jboolean {
    with_env(&mut env, JNI_FALSE, |_env| {
        let (_, info) = model_info(handle)?;
        Ok(jboolean::from(info.produces_normalized_embeddings))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_runtimeVersion(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jstring {
    with_env(&mut env, ptr::null_mut(), |env| {
        let _ = embedder(handle)?;
        new_string(env, ONNX_RUNTIME_VERSION)
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_embedding_internal_NativeBridge_close(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    with_env(&mut env, (), |_env| close_embedder(handle));
}

fn validate_model_info(info: &EmbeddingModelInfo) -> BoundaryResult<()> {
    if info.profile != EmbeddingProfile::Fp32 {
        return Err(BoundaryError {
            class: MODEL_LOAD_EXCEPTION,
            message: format!(
                "Kotlin production embedding requires fp32, found {}",
                info.profile
            ),
        });
    }
    if info.dimension != EMBEDDING_DIMENSION
        || !info.produces_normalized_embeddings
        || info.max_input_tokens != retrievalkit_embedding::MAX_INPUT_TOKENS
    {
        return Err(BoundaryError {
            class: MODEL_LOAD_EXCEPTION,
            message: format!(
                "unsupported embedding contract: dimension={}, maxInputTokens={}, normalized={}",
                info.dimension, info.max_input_tokens, info.produces_normalized_embeddings
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_error_categories_are_stable() {
        assert_eq!(
            BoundaryError::embedding(EmbeddingError::EmptyInput, Operation::Inference).class,
            INVALID_INPUT_EXCEPTION
        );
        assert_eq!(
            BoundaryError::embedding(
                EmbeddingError::ModelUnavailable(PathBuf::from("missing")),
                Operation::Load
            )
            .class,
            MODEL_ACQUISITION_EXCEPTION
        );
        assert_eq!(
            BoundaryError::embedding(
                EmbeddingError::InvalidManifest("invalid".into()),
                Operation::Acquisition
            )
            .class,
            MODEL_INTEGRITY_EXCEPTION
        );
        assert_eq!(
            BoundaryError::embedding(EmbeddingError::Onnx("load".into()), Operation::Load).class,
            MODEL_LOAD_EXCEPTION
        );
        assert_eq!(
            BoundaryError::embedding(EmbeddingError::Onnx("run".into()), Operation::Inference)
                .class,
            INFERENCE_EXCEPTION
        );
    }

    #[test]
    fn invalid_handles_use_the_closed_category() {
        let error = embedder(0).unwrap_err();
        assert_eq!(error.class, CLOSED_EXCEPTION);
        assert!(error.message.contains("closed or invalid"));
    }

    #[test]
    fn thread_counts_must_be_positive() {
        assert!(validate_threads(0, "intraThreads").is_err());
        assert!(validate_threads(-1, "interThreads").is_err());
        assert_eq!(validate_threads(1, "intraThreads").unwrap(), 1);
    }

    #[test]
    fn output_validation_requires_the_public_contract() {
        assert!(validate_output(&[0.0; EMBEDDING_DIMENSION]).is_err());
        let mut normalized = vec![0.0; EMBEDDING_DIMENSION];
        normalized[0] = 1.0;
        assert!(validate_output(&normalized).is_ok());
        normalized[0] = f32::NAN;
        assert!(validate_output(&normalized).is_err());
    }
}
