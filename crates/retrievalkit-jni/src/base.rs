use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use jni::objects::{JClass, JFloatArray, JLongArray, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jfloat, jint, jlong, JNI_FALSE};
use jni::JNIEnv;
use retrievalkit_core::{
    Bm25Config, Filter, HybridFusionTrace, HybridHit, HybridQuery, IndexPersistenceOptions,
    KeywordHit, KeywordQuery, Metadata, MetadataValue, RetrievalDatabase, RetrievalDatabaseBuilder,
    RetrievalKitError, SearchHit, SearchQuery, VectorEncoding, VectorMetric,
};

#[cfg(feature = "graph")]
use retrievalkit_graph::{
    GraphDatabase, GraphDatabaseBuilder, GraphResult, GraphRetrievalDatabase,
    GraphRetrievalDatabaseBuilder,
};

pub(crate) type BoundaryResult<T> = Result<T, BoundaryError>;

#[derive(Debug)]
pub(crate) struct BoundaryError {
    pub(crate) class: &'static str,
    pub(crate) message: String,
}

impl BoundaryError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            class: "ai/retrievalkit/InvalidQueryException",
            message: message.into(),
        }
    }

    pub(crate) fn native(message: impl Into<String>) -> Self {
        Self {
            class: "ai/retrievalkit/NativeLibraryException",
            message: message.into(),
        }
    }
}

impl From<jni::errors::Error> for BoundaryError {
    fn from(error: jni::errors::Error) -> Self {
        Self::native(format!("JNI conversion failed: {error}"))
    }
}

impl From<RetrievalKitError> for BoundaryError {
    fn from(error: RetrievalKitError) -> Self {
        let class = match error {
            RetrievalKitError::InvalidIdentity { .. }
            | RetrievalKitError::InvalidRecordValue { .. } => {
                "ai/retrievalkit/InvalidIdentityException"
            }
            RetrievalKitError::InvalidDimension { .. } => {
                "ai/retrievalkit/InvalidDimensionException"
            }
            RetrievalKitError::MissingEmbedding { .. } => {
                "ai/retrievalkit/MissingEmbeddingException"
            }
            RetrievalKitError::InvalidRange { .. } => "ai/retrievalkit/InvalidFilterException",
            RetrievalKitError::Persistence { .. } => "ai/retrievalkit/PersistenceException",
            RetrievalKitError::CorruptIndex { .. } | RetrievalKitError::InvalidFormat { .. } => {
                "ai/retrievalkit/CorruptIndexException"
            }
            RetrievalKitError::StaleGeneration { .. }
            | RetrievalKitError::InvalidCandidateScope { .. } => {
                "ai/retrievalkit/StaleSelectionException"
            }
            RetrievalKitError::InvalidQuery { .. }
            | RetrievalKitError::UnsupportedVectorEncoding { .. }
            | RetrievalKitError::RetrievalCapabilityUnavailable { .. } => {
                "ai/retrievalkit/InvalidQueryException"
            }
        };
        Self {
            class,
            message: error.to_string(),
        }
    }
}

pub(crate) enum Resource {
    Closed,
    RetrievalBuilder(Box<RetrievalDatabaseBuilder>),
    Retrieval(Box<RetrievalDatabase>),
    #[cfg(feature = "graph")]
    GraphBuilder(Box<GraphDatabaseBuilder>),
    #[cfg(feature = "graph")]
    Graph(Box<GraphDatabase>),
    #[cfg(feature = "graph")]
    GraphRetrievalBuilder(Box<GraphRetrievalDatabaseBuilder>),
    #[cfg(feature = "graph")]
    GraphRetrieval(Box<GraphRetrievalDatabase>),
    #[cfg(feature = "graph")]
    Selection(GraphResult),
}

static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);
type SharedResource = Arc<Mutex<Resource>>;

static RESOURCES: LazyLock<Mutex<HashMap<jlong, SharedResource>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn resources() -> BoundaryResult<MutexGuard<'static, HashMap<jlong, SharedResource>>> {
    RESOURCES
        .lock()
        .map_err(|_| BoundaryError::native("native handle registry is poisoned"))
}

pub(crate) fn insert_resource(resource: Resource) -> BoundaryResult<jlong> {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    if handle <= 0 {
        return Err(BoundaryError::native("native handle space is exhausted"));
    }
    resources()?.insert(handle, Arc::new(Mutex::new(resource)));
    Ok(handle)
}

pub(crate) fn resource(handle: jlong) -> BoundaryResult<SharedResource> {
    resources()?.get(&handle).cloned().ok_or_else(|| {
        BoundaryError::invalid(format!("native handle {handle} is closed or invalid"))
    })
}

pub(crate) fn remove_resource(handle: jlong) -> BoundaryResult<SharedResource> {
    resources()?.remove(&handle).ok_or_else(|| {
        BoundaryError::invalid(format!("native handle {handle} is closed or invalid"))
    })
}

pub(crate) fn lock_resource(resource: &SharedResource) -> BoundaryResult<MutexGuard<'_, Resource>> {
    resource
        .lock()
        .map_err(|_| BoundaryError::native("native database state is poisoned"))
}

pub(crate) fn with_env<'local, T: Copy>(
    env: &mut JNIEnv<'local>,
    default: T,
    operation: impl FnOnce(&mut JNIEnv<'local>) -> BoundaryResult<T>,
) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(env))) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let _ = env.throw_new(error.class, error.message);
            default
        }
        Err(_) => {
            let _ = env.throw_new(
                "ai/retrievalkit/NativeLibraryException",
                "native RetrievalKit operation panicked; close the database and report this bug",
            );
            default
        }
    }
}

pub(crate) fn with_env_object<'local>(
    env: &mut JNIEnv<'local>,
    operation: impl FnOnce(&mut JNIEnv<'local>) -> BoundaryResult<JObject<'local>>,
) -> JObject<'local> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(env))) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let _ = env.throw_new(error.class, error.message);
            JObject::null()
        }
        Err(_) => {
            let _ = env.throw_new(
                "ai/retrievalkit/NativeLibraryException",
                "native RetrievalKit operation panicked; close the database and report this bug",
            );
            JObject::null()
        }
    }
}

pub(crate) fn string<'local, 'object>(
    env: &mut JNIEnv<'local>,
    value: &JObject<'object>,
) -> BoundaryResult<String> {
    if value.is_null() {
        return Err(BoundaryError::invalid("required string was null"));
    }
    Ok(env
        .get_string(<&JString>::from(value))
        .map_err(BoundaryError::from)?
        .into())
}

pub(crate) fn method_object<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
    name: &str,
    signature: &str,
) -> BoundaryResult<JObject<'local>> {
    env.call_method(object, name, signature, &[])?
        .l()
        .map_err(BoundaryError::from)
}

pub(crate) fn method_int<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
    name: &str,
) -> BoundaryResult<jint> {
    Ok(env.call_method(object, name, "()I", &[])?.i()?)
}

pub(crate) fn java_list<'local>(
    env: &mut JNIEnv<'local>,
    value: &JObject<'local>,
) -> BoundaryResult<Vec<JObject<'local>>> {
    let size = env.call_method(value, "size", "()I", &[])?.i()?;
    if size < 0 {
        return Err(BoundaryError::invalid("Java list reported a negative size"));
    }
    let mut values = Vec::with_capacity(size as usize);
    for index in 0..size {
        values.push(
            env.call_method(value, "get", "(I)Ljava/lang/Object;", &[JValue::Int(index)])?
                .l()?,
        );
    }
    Ok(values)
}

pub(crate) fn metadata<'local, 'object>(
    env: &mut JNIEnv<'local>,
    entries: &JObject<'object>,
) -> BoundaryResult<Metadata> {
    let entries = <&JObjectArray>::from(entries);
    let len = env.get_array_length(entries)?;
    let mut metadata = BTreeMap::new();
    for index in 0..len {
        let entry = env.get_object_array_element(entries, index)?;
        let key_object = method_object(env, &entry, "getKey", "()Ljava/lang/String;")?;
        let key = string(env, &key_object)?;
        let value_type = method_int(env, &entry, "getType")?;
        let value = match value_type {
            0 => {
                let value = method_object(env, &entry, "getStringValue", "()Ljava/lang/String;")?;
                MetadataValue::String(string(env, &value)?)
            }
            1 => MetadataValue::Integer(env.call_method(&entry, "getLongValue", "()J", &[])?.j()?),
            2 => MetadataValue::Float(env.call_method(&entry, "getDoubleValue", "()D", &[])?.d()?),
            3 => MetadataValue::Boolean(
                env.call_method(&entry, "getBooleanValue", "()Z", &[])?
                    .z()?,
            ),
            4 => MetadataValue::TimestampMillis(
                env.call_method(&entry, "getLongValue", "()J", &[])?.j()?,
            ),
            other => {
                return Err(BoundaryError::invalid(format!(
                    "metadata entry '{key}' has unsupported native type {other}"
                )))
            }
        };
        metadata.insert(key, value);
    }
    Ok(metadata)
}

pub(crate) fn metadata_value<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<MetadataValue> {
    if env.is_instance_of(object, "ai/retrievalkit/MetadataValue$Text")? {
        let value = method_object(env, object, "getValue", "()Ljava/lang/String;")?;
        Ok(MetadataValue::String(string(env, &value)?))
    } else if env.is_instance_of(object, "ai/retrievalkit/MetadataValue$Integer")? {
        Ok(MetadataValue::Integer(
            env.call_method(object, "getValue", "()J", &[])?.j()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/MetadataValue$Decimal")? {
        Ok(MetadataValue::Float(
            env.call_method(object, "getValue", "()D", &[])?.d()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/MetadataValue$Boolean")? {
        Ok(MetadataValue::Boolean(
            env.call_method(object, "getValue", "()Z", &[])?.z()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/MetadataValue$TimestampMillis")? {
        Ok(MetadataValue::TimestampMillis(
            env.call_method(object, "getValue", "()J", &[])?.j()?,
        ))
    } else {
        Err(BoundaryError::invalid(
            "metadata value has an unsupported Kotlin subtype",
        ))
    }
}

pub(crate) fn filter<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<Option<Filter>> {
    if object.is_null() {
        return Ok(None);
    }
    let field_value = |env: &mut JNIEnv<'local>, object: &JObject<'object>| {
        let field = method_object(env, object, "getField", "()Ljava/lang/String;")?;
        let value = method_object(env, object, "getValue", "()Lai/retrievalkit/MetadataValue;")?;
        Ok::<_, BoundaryError>((string(env, &field)?, metadata_value(env, &value)?))
    };
    let parsed = if env.is_instance_of(object, "ai/retrievalkit/Filter$Equals")? {
        let (field, value) = field_value(env, object)?;
        Filter::Equals { field, value }
    } else if env.is_instance_of(object, "ai/retrievalkit/Filter$NotEquals")? {
        let (field, value) = field_value(env, object)?;
        Filter::NotEquals { field, value }
    } else if env.is_instance_of(object, "ai/retrievalkit/Filter$In")? {
        let field_object = method_object(env, object, "getField", "()Ljava/lang/String;")?;
        let values_object = method_object(env, object, "getValues", "()Ljava/util/List;")?;
        let values = java_list(env, &values_object)?
            .iter()
            .map(|value| metadata_value(env, value))
            .collect::<BoundaryResult<Vec<_>>>()?;
        Filter::In {
            field: string(env, &field_object)?,
            values,
        }
    } else if env.is_instance_of(object, "ai/retrievalkit/Filter$Range")? {
        let field_object = method_object(env, object, "getField", "()Ljava/lang/String;")?;
        let lower = method_object(env, object, "getLower", "()Lai/retrievalkit/MetadataValue;")?;
        let upper = method_object(env, object, "getUpper", "()Lai/retrievalkit/MetadataValue;")?;
        Filter::Range {
            field: string(env, &field_object)?,
            lower: (!lower.is_null())
                .then(|| metadata_value(env, &lower))
                .transpose()?,
            upper: (!upper.is_null())
                .then(|| metadata_value(env, &upper))
                .transpose()?,
        }
    } else if env.is_instance_of(object, "ai/retrievalkit/Filter$Exists")? {
        let field = method_object(env, object, "getField", "()Ljava/lang/String;")?;
        Filter::Exists {
            field: string(env, &field)?,
        }
    } else {
        let (class, getter) = if env.is_instance_of(object, "ai/retrievalkit/Filter$All")? {
            (true, "getFilters")
        } else if env.is_instance_of(object, "ai/retrievalkit/Filter$AnyOf")? {
            (false, "getFilters")
        } else {
            return Err(BoundaryError::invalid(
                "filter has an unsupported Kotlin subtype",
            ));
        };
        let children = method_object(env, object, getter, "()Ljava/util/List;")?;
        let children = java_list(env, &children)?
            .iter()
            .map(|child| {
                filter(env, child)?
                    .ok_or_else(|| BoundaryError::invalid("composite filter contains a null child"))
            })
            .collect::<BoundaryResult<Vec<_>>>()?;
        if class {
            Filter::All(children)
        } else {
            Filter::Any(children)
        }
    };
    Ok(Some(parsed))
}

pub(crate) fn vector_metric(value: jint) -> BoundaryResult<VectorMetric> {
    match value {
        0 => Ok(VectorMetric::DotProduct),
        1 => Ok(VectorMetric::Cosine),
        _ => Err(BoundaryError::invalid(format!(
            "vector metric ordinal {value} is unsupported"
        ))),
    }
}

pub(crate) fn vector_encoding(value: jint) -> BoundaryResult<VectorEncoding> {
    match value {
        0 => Ok(VectorEncoding::F32),
        1 => Ok(VectorEncoding::F16),
        2 => Ok(VectorEncoding::BF16),
        3 => Ok(VectorEncoding::I8ScalarQuantized),
        _ => Err(BoundaryError::invalid(format!(
            "vector encoding ordinal {value} is unsupported"
        ))),
    }
}

pub(crate) fn float_array<'local, 'array>(
    env: &mut JNIEnv<'local>,
    array: &JFloatArray<'array>,
) -> BoundaryResult<Vec<f32>> {
    if array.is_null() {
        return Err(BoundaryError::invalid("embedding was null"));
    }
    let len = env.get_array_length(array)?;
    let mut values = vec![0.0; len as usize];
    env.get_float_array_region(array, 0, &mut values)?;
    Ok(values)
}

fn optional_float_array<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<Vec<f32>> {
    if object.is_null() {
        Ok(Vec::new())
    } else {
        float_array(env, <&JFloatArray>::from(object))
    }
}

fn positive_limit(name: &'static str, value: jint) -> BoundaryResult<usize> {
    usize::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            BoundaryError::invalid(format!("{name} must be greater than zero; got {value}"))
        })
}

pub(crate) fn strings_from_array(
    env: &mut JNIEnv<'_>,
    values: &JObjectArray<'_>,
) -> BoundaryResult<Vec<String>> {
    let length = env.get_array_length(values)?;
    let mut output = Vec::with_capacity(length as usize);
    for index in 0..length {
        let value = env.get_object_array_element(values, index)?;
        output.push(string(env, &value)?);
    }
    Ok(output)
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_createRetrievalBuilder(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    corpus_id: JString<'_>,
    metric: jint,
    encoding: jint,
    bm25_k1: jfloat,
    bm25_b: jfloat,
    stop_words: JObjectArray<'_>,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let corpus_id = string(env, &JObject::from(corpus_id))?;
        let builder = RetrievalDatabaseBuilder::new(
            retrievalkit_core::CorpusId::new(corpus_id)?,
            vector_metric(metric)?,
            vector_encoding(encoding)?,
        )
        .try_with_bm25_config(Bm25Config::try_new(
            bm25_k1,
            bm25_b,
            strings_from_array(env, &stop_words)?,
        )?)?;
        insert_resource(Resource::RetrievalBuilder(Box::new(builder)))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_retrievalBuilderUpsert(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    document: JObject<'_>,
    embedding: JFloatArray<'_>,
) {
    with_env(&mut env, (), |env| {
        let id_object = method_object(env, &document, "getId", "()Ljava/lang/String;")?;
        let text_object = method_object(env, &document, "getText", "()Ljava/lang/String;")?;
        let metadata_object = method_object(
            env,
            &document,
            "getMetadata",
            "()[Lai/retrievalkit/internal/NativeMetadataEntry;",
        )?;
        let document = retrievalkit_core::Document {
            id: string(env, &id_object)?,
            text: string(env, &text_object)?,
            metadata: metadata(env, &metadata_object)?,
        };
        let embedding = float_array(env, &embedding)?;
        let resource = resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::RetrievalBuilder(builder) = &mut *resource else {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} is not a RetrievalDatabase.Builder"
            )));
        };
        builder.upsert_document(document, embedding)?;
        Ok(())
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_buildRetrieval(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jlong {
    with_env(&mut env, 0, |_env| {
        let resource = remove_resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::RetrievalBuilder(builder) =
            std::mem::replace(&mut *resource, Resource::Closed)
        else {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} is not a RetrievalDatabase.Builder"
            )));
        };
        insert_resource(Resource::Retrieval(Box::new((*builder).build()?)))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_loadRetrieval(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let path = string(env, &JObject::from(path))?;
        insert_resource(Resource::Retrieval(Box::new(
            RetrievalDatabase::load_from_dir(&path)?,
        )))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_validateRetrieval(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) {
    with_env(&mut env, (), |env| {
        let path = string(env, &JObject::from(path))?;
        RetrievalDatabase::validate_dir(path)?;
        Ok(())
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_retrievalDimension(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jint {
    with_env(&mut env, 0, |_env| {
        let resource = resource(handle)?;
        let resource = lock_resource(&resource)?;
        match &*resource {
            Resource::Retrieval(database) => i32::try_from(database.retrieval().dimension())
                .map_err(|_| BoundaryError::native("retrieval dimension exceeds JVM Int")),
            #[cfg(feature = "graph")]
            Resource::GraphRetrieval(database) => {
                i32::try_from(database.retrieval().retrieval().dimension())
                    .map_err(|_| BoundaryError::native("retrieval dimension exceeds JVM Int"))
            }
            _ => Err(BoundaryError::invalid(format!(
                "native handle {handle} has no retrieval capability"
            ))),
        }
    })
}

#[cfg(feature = "graph")]
fn selection_result(handle: jlong) -> BoundaryResult<retrievalkit_graph::GraphResult> {
    let resource = resource(handle)?;
    let resource = lock_resource(&resource)?;
    let Resource::Selection(selection) = &*resource else {
        return Err(BoundaryError::invalid(format!(
            "graph selection native handle {handle} is closed or invalid"
        )));
    };
    Ok(selection.clone())
}

type HitPayload = (String, String, String, Metadata);
type SearchResponse = (Vec<SearchHit>, Vec<HitPayload>);
type KeywordResponse = (Vec<KeywordHit>, Vec<HitPayload>);
type HybridResponse = (Vec<HybridHit>, Vec<HitPayload>);

fn run_semantic(
    handle: jlong,
    query: &SearchQuery,
    selection_handle: jlong,
) -> BoundaryResult<SearchResponse> {
    #[cfg(feature = "graph")]
    let selection = (selection_handle != 0)
        .then(|| selection_result(selection_handle))
        .transpose()?;
    #[cfg(not(feature = "graph"))]
    if selection_handle != 0 {
        return Err(BoundaryError::invalid(
            "the graph-free native aggregate cannot accept graph selections",
        ));
    }
    let resource = resource(handle)?;
    let resource = lock_resource(&resource)?;
    let (hits, corpus) = match &*resource {
        Resource::Retrieval(database) if selection_handle == 0 => {
            (database.semantic_search(query)?, database.corpus())
        }
        #[cfg(feature = "graph")]
        Resource::GraphRetrieval(database) => {
            let hits = match selection {
                Some(selection) => database.semantic_search_in_selection(query, &selection)?,
                None => database.semantic_search(query)?,
            };
            (hits, database.corpus())
        }
        Resource::Retrieval(_) => {
            return Err(BoundaryError::invalid(
                "a graph selection cannot scope a base RetrievalDatabase",
            ))
        }
        _ => {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} has no retrieval capability"
            )))
        }
    };
    let mut payloads = Vec::with_capacity(hits.len());
    for hit in &hits {
        let chunk = corpus.chunk(hit.chunk_id).ok_or_else(|| {
            BoundaryError::native(format!(
                "ranked chunk {} could not be hydrated from the canonical corpus",
                hit.chunk_id
            ))
        })?;
        let identity = corpus.chunk_identity(hit.chunk_id).ok_or_else(|| {
            BoundaryError::native(format!(
                "ranked chunk {} has no stable canonical identity",
                hit.chunk_id
            ))
        })?;
        payloads.push((
            identity.record_id.as_str().to_owned(),
            identity.chunk_key.as_str().to_owned(),
            chunk.text.clone(),
            chunk.metadata.clone(),
        ));
    }
    Ok((hits, payloads))
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_semanticSearch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    embedding: JFloatArray<'local>,
    limit: jint,
    filter_object: JObject<'local>,
    selection_handle: jlong,
) -> JObject<'local> {
    with_env_object(&mut env, |env| {
        let embedding = float_array(env, &embedding)?;
        let mut query = SearchQuery::new(embedding, positive_limit("limit", limit)?);
        if let Some(filter) = filter(env, &filter_object)? {
            query = query.with_filter(filter);
        }
        let (hits, payloads) = run_semantic(handle, &query, selection_handle)?;
        search_results(env, &hits, &payloads)
    })
}

fn run_keyword(
    handle: jlong,
    query: &KeywordQuery,
    selection_handle: jlong,
) -> BoundaryResult<KeywordResponse> {
    #[cfg(feature = "graph")]
    let selection = (selection_handle != 0)
        .then(|| selection_result(selection_handle))
        .transpose()?;
    #[cfg(not(feature = "graph"))]
    if selection_handle != 0 {
        return Err(BoundaryError::invalid(
            "the graph-free native aggregate cannot accept graph selections",
        ));
    }
    let resource = resource(handle)?;
    let resource = lock_resource(&resource)?;
    let (hits, corpus) = match &*resource {
        Resource::Retrieval(database) if selection_handle == 0 => {
            (database.keyword_search(query)?, database.corpus())
        }
        #[cfg(feature = "graph")]
        Resource::GraphRetrieval(database) => {
            let hits = match selection {
                Some(selection) => database.keyword_search_in_selection(query, &selection)?,
                None => database.keyword_search(query)?,
            };
            (hits, database.corpus())
        }
        Resource::Retrieval(_) => {
            return Err(BoundaryError::invalid(
                "a graph selection cannot scope a base RetrievalDatabase",
            ))
        }
        _ => {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} has no retrieval capability"
            )))
        }
    };
    let mut payloads = Vec::with_capacity(hits.len());
    for hit in &hits {
        let chunk = corpus.chunk(hit.chunk_id).ok_or_else(|| {
            BoundaryError::native(format!(
                "ranked chunk {} could not be hydrated from the canonical corpus",
                hit.chunk_id
            ))
        })?;
        let identity = corpus.chunk_identity(hit.chunk_id).ok_or_else(|| {
            BoundaryError::native(format!(
                "ranked chunk {} has no stable canonical identity",
                hit.chunk_id
            ))
        })?;
        payloads.push((
            identity.record_id.as_str().to_owned(),
            identity.chunk_key.as_str().to_owned(),
            chunk.text.clone(),
            chunk.metadata.clone(),
        ));
    }
    Ok((hits, payloads))
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_keywordSearch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    text: JString<'local>,
    limit: jint,
    filter_object: JObject<'local>,
    selection_handle: jlong,
) -> JObject<'local> {
    with_env_object(&mut env, |env| {
        let text = string(env, &JObject::from(text))?;
        let mut query = KeywordQuery::new(text, positive_limit("limit", limit)?);
        if let Some(filter) = filter(env, &filter_object)? {
            query = query.with_filter(filter);
        }
        let (hits, payloads) = run_keyword(handle, &query, selection_handle)?;
        keyword_results(env, &hits, &payloads)
    })
}

fn run_hybrid(
    handle: jlong,
    query: &HybridQuery,
    selection_handle: jlong,
) -> BoundaryResult<HybridResponse> {
    #[cfg(feature = "graph")]
    let selection = (selection_handle != 0)
        .then(|| selection_result(selection_handle))
        .transpose()?;
    #[cfg(not(feature = "graph"))]
    if selection_handle != 0 {
        return Err(BoundaryError::invalid(
            "the graph-free native aggregate cannot accept graph selections",
        ));
    }
    let resource = resource(handle)?;
    let resource = lock_resource(&resource)?;
    let (hits, corpus) = match &*resource {
        Resource::Retrieval(database) if selection_handle == 0 => {
            (database.hybrid_search(query)?, database.corpus())
        }
        #[cfg(feature = "graph")]
        Resource::GraphRetrieval(database) => {
            let hits = match selection {
                Some(selection) => database.hybrid_search_in_selection(query, &selection)?,
                None => database.hybrid_search(query)?,
            };
            (hits, database.corpus())
        }
        Resource::Retrieval(_) => {
            return Err(BoundaryError::invalid(
                "a graph selection cannot scope a base RetrievalDatabase",
            ))
        }
        _ => {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} has no retrieval capability"
            )))
        }
    };
    let mut payloads = Vec::with_capacity(hits.len());
    for hit in &hits {
        let chunk = corpus.chunk(hit.chunk_id).ok_or_else(|| {
            BoundaryError::native(format!(
                "ranked chunk {} could not be hydrated from the canonical corpus",
                hit.chunk_id
            ))
        })?;
        let identity = corpus.chunk_identity(hit.chunk_id).ok_or_else(|| {
            BoundaryError::native(format!(
                "ranked chunk {} has no stable canonical identity",
                hit.chunk_id
            ))
        })?;
        payloads.push((
            identity.record_id.as_str().to_owned(),
            identity.chunk_key.as_str().to_owned(),
            chunk.text.clone(),
            chunk.metadata.clone(),
        ));
    }
    Ok((hits, payloads))
}

#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_hybridSearch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    text: JString<'local>,
    embedding: JObject<'local>,
    limit: jint,
    alpha: jfloat,
    filter_object: JObject<'local>,
    vector_candidates: jint,
    keyword_candidates: jint,
    selection_handle: jlong,
) -> JObject<'local> {
    with_env_object(&mut env, |env| {
        let text = string(env, &JObject::from(text))?;
        let embedding = optional_float_array(env, &embedding)?;
        let mut query = HybridQuery::new(text, embedding, positive_limit("limit", limit)?)
            .with_candidate_limits(
                positive_limit("vectorCandidates", vector_candidates)?,
                positive_limit("keywordCandidates", keyword_candidates)?,
            )
            .try_with_alpha(alpha)?;
        if let Some(filter) = filter(env, &filter_object)? {
            query = query.with_filter(filter);
        }
        let (hits, payloads) = run_hybrid(handle, &query, selection_handle)?;
        hybrid_results(env, &hits, &payloads)
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_deleteRecord(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    record_id: JString<'_>,
) -> jint {
    with_env(&mut env, 0, |env| {
        let record_id = retrievalkit_core::RecordId::new(string(env, &JObject::from(record_id))?)?;
        let resource = resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let deleted = match &mut *resource {
            Resource::Retrieval(database) => database.delete_record(&record_id),
            _ => {
                return Err(BoundaryError::invalid(format!(
                    "native handle {handle} is not a mutable RetrievalDatabase"
                )))
            }
        };
        i32::try_from(deleted)
            .map_err(|_| BoundaryError::native("deleted record count exceeds JVM Int"))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_saveRetrieval(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    path: JString<'_>,
    include_bm25: jboolean,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let path = string(env, &JObject::from(path))?;
        let resource = resource(handle)?;
        let resource = lock_resource(&resource)?;
        let Resource::Retrieval(database) = &*resource else {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} is not a RetrievalDatabase"
            )));
        };
        let options = if include_bm25 == JNI_FALSE {
            IndexPersistenceOptions::vector_only()
        } else {
            IndexPersistenceOptions::hybrid()
        };
        let report = database
            .as_compatibility_index()
            .save_to_dir_with_options(Path::new(&path), options)?;
        i64::try_from(report.total_bytes())
            .map_err(|_| BoundaryError::native("persistence size exceeds JVM Long"))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_compact<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JLongArray<'local> {
    let object = with_env_object(&mut env, |env| {
        let resource = resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::Retrieval(database) = &mut *resource else {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} is not a RetrievalDatabase"
            )));
        };
        let report = database.compact()?;
        let values = [
            report.chunks_before as i64,
            report.chunks_after as i64,
            report.chunks_removed as i64,
            report.estimated_bytes_reclaimed as i64,
        ];
        let array = env.new_long_array(values.len() as i32)?;
        env.set_long_array_region(&array, 0, &values)?;
        Ok(JObject::from(array))
    });
    JLongArray::from(object)
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_closeHandle(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    with_env(&mut env, (), |_env| {
        let Some(resource) = resources()?.remove(&handle) else {
            return Ok(());
        };
        let mut resource = lock_resource(&resource)?;
        *resource = Resource::Closed;
        Ok(())
    });
}

fn metadata_objects<'local>(
    env: &mut JNIEnv<'local>,
    metadata: &Metadata,
) -> BoundaryResult<JObjectArray<'local>> {
    let class = env.find_class("ai/retrievalkit/internal/NativeMetadataEntry")?;
    let array = env.new_object_array(metadata.len() as i32, &class, JObject::null())?;
    for (index, (key, value)) in metadata.iter().enumerate() {
        let key = env.new_string(key)?;
        let (value_type, string_value, long_value, double_value, boolean_value) = match value {
            MetadataValue::String(value) => {
                (0, JObject::from(env.new_string(value)?), 0, 0.0, false)
            }
            MetadataValue::Integer(value) => (1, JObject::null(), *value, 0.0, false),
            MetadataValue::Float(value) => (2, JObject::null(), 0, *value, false),
            MetadataValue::Boolean(value) => (3, JObject::null(), 0, 0.0, *value),
            MetadataValue::TimestampMillis(value) => (4, JObject::null(), *value, 0.0, false),
        };
        let key_object = JObject::from(key);
        let entry = env.new_object(
            &class,
            "(Ljava/lang/String;ILjava/lang/String;JDZ)V",
            &[
                JValue::Object(&key_object),
                JValue::Int(value_type),
                JValue::Object(&string_value),
                JValue::Long(long_value),
                JValue::Double(double_value),
                JValue::Bool(boolean_value.into()),
            ],
        )?;
        env.set_object_array_element(&array, index as i32, entry)?;
    }
    Ok(array)
}

fn search_results<'local>(
    env: &mut JNIEnv<'local>,
    hits: &[SearchHit],
    payloads: &[(String, String, String, Metadata)],
) -> BoundaryResult<JObject<'local>> {
    let class = env.find_class("ai/retrievalkit/internal/NativeSearchHit")?;
    let array = env.new_object_array(hits.len() as i32, &class, JObject::null())?;
    for (index, (hit, payload)) in hits.iter().zip(payloads).enumerate() {
        let record_id = JObject::from(env.new_string(&payload.0)?);
        let chunk_key = JObject::from(env.new_string(&payload.1)?);
        let text = JObject::from(env.new_string(&payload.2)?);
        let metadata = JObject::from(metadata_objects(env, &payload.3)?);
        let object = env.new_object(
            &class,
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;FF[Lai/retrievalkit/internal/NativeMetadataEntry;)V",
            &[
                JValue::Object(&record_id),
                JValue::Object(&chunk_key),
                JValue::Object(&text),
                JValue::Float(hit.score),
                JValue::Float(hit.trace.vector_score),
                JValue::Object(&metadata),
            ],
        )?;
        env.set_object_array_element(&array, index as i32, object)?;
    }
    Ok(JObject::from(array))
}

fn keyword_results<'local>(
    env: &mut JNIEnv<'local>,
    hits: &[KeywordHit],
    payloads: &[(String, String, String, Metadata)],
) -> BoundaryResult<JObject<'local>> {
    let class = env.find_class("ai/retrievalkit/internal/NativeKeywordHit")?;
    let array = env.new_object_array(hits.len() as i32, &class, JObject::null())?;
    for (index, (hit, payload)) in hits.iter().zip(payloads).enumerate() {
        let record_id = JObject::from(env.new_string(&payload.0)?);
        let chunk_key = JObject::from(env.new_string(&payload.1)?);
        let text = JObject::from(env.new_string(&payload.2)?);
        let metadata = JObject::from(metadata_objects(env, &payload.3)?);
        let matched_terms = JObject::from(string_array(env, &hit.matched_terms)?);
        let object = env.new_object(
            &class,
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;F[Lai/retrievalkit/internal/NativeMetadataEntry;[Ljava/lang/String;)V",
            &[
                JValue::Object(&record_id),
                JValue::Object(&chunk_key),
                JValue::Object(&text),
                JValue::Float(hit.score),
                JValue::Object(&metadata),
                JValue::Object(&matched_terms),
            ],
        )?;
        env.set_object_array_element(&array, index as i32, object)?;
    }
    Ok(JObject::from(array))
}

fn boxed_float<'local>(
    env: &mut JNIEnv<'local>,
    value: Option<f32>,
) -> BoundaryResult<JObject<'local>> {
    match value {
        Some(value) => Ok(env.new_object("java/lang/Float", "(F)V", &[JValue::Float(value)])?),
        None => Ok(JObject::null()),
    }
}

fn boxed_int<'local>(
    env: &mut JNIEnv<'local>,
    value: Option<usize>,
) -> BoundaryResult<JObject<'local>> {
    match value {
        Some(value) => Ok(env.new_object(
            "java/lang/Integer",
            "(I)V",
            &[JValue::Int(i32::try_from(value).map_err(|_| {
                BoundaryError::native("result rank exceeds JVM Int")
            })?)],
        )?),
        None => Ok(JObject::null()),
    }
}

pub(crate) fn string_array<'local>(
    env: &mut JNIEnv<'local>,
    values: &[String],
) -> BoundaryResult<JObjectArray<'local>> {
    let class = env.find_class("java/lang/String")?;
    let array = env.new_object_array(values.len() as i32, class, JObject::null())?;
    for (index, value) in values.iter().enumerate() {
        let value = env.new_string(value)?;
        env.set_object_array_element(&array, index as i32, value)?;
    }
    Ok(array)
}

fn hybrid_results<'local>(
    env: &mut JNIEnv<'local>,
    hits: &[HybridHit],
    payloads: &[(String, String, String, Metadata)],
) -> BoundaryResult<JObject<'local>> {
    let class = env.find_class("ai/retrievalkit/internal/NativeHybridHit")?;
    let array = env.new_object_array(hits.len() as i32, &class, JObject::null())?;
    for (index, (hit, payload)) in hits.iter().zip(payloads).enumerate() {
        let record_id = JObject::from(env.new_string(&payload.0)?);
        let chunk_key = JObject::from(env.new_string(&payload.1)?);
        let text = JObject::from(env.new_string(&payload.2)?);
        let vector_score = boxed_float(env, hit.vector_score)?;
        let keyword_score = boxed_float(env, hit.keyword_score)?;
        let metadata = JObject::from(metadata_objects(env, &payload.3)?);
        let vector_rank = boxed_int(env, hit.trace.vector_rank)?;
        let keyword_rank = boxed_int(env, hit.trace.keyword_rank)?;
        let normalized_vector = boxed_float(env, hit.trace.normalized_vector_score)?;
        let normalized_keyword = boxed_float(env, hit.trace.normalized_keyword_score)?;
        let matched_terms = JObject::from(string_array(env, &hit.trace.matched_terms)?);
        let alpha = match hit.trace.fusion {
            HybridFusionTrace::WeightedNormalizedScore { vector_weight, .. } => vector_weight,
            HybridFusionTrace::ReciprocalRank { .. } => {
                return Err(BoundaryError::native(
                    "public Kotlin result received an internal RRF trace",
                ))
            }
        };
        let object = env.new_object(
            &class,
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;FLjava/lang/Float;Ljava/lang/Float;[Lai/retrievalkit/internal/NativeMetadataEntry;Ljava/lang/Integer;Ljava/lang/Integer;Ljava/lang/Float;Ljava/lang/Float;[Ljava/lang/String;F)V",
            &[
                JValue::Object(&record_id),
                JValue::Object(&chunk_key),
                JValue::Object(&text),
                JValue::Float(hit.score),
                JValue::Object(&vector_score),
                JValue::Object(&keyword_score),
                JValue::Object(&metadata),
                JValue::Object(&vector_rank),
                JValue::Object(&keyword_rank),
                JValue::Object(&normalized_vector),
                JValue::Object(&normalized_keyword),
                JValue::Object(&matched_terms),
                JValue::Float(alpha),
            ],
        )?;
        env.set_object_array_element(&array, index as i32, object)?;
    }
    Ok(JObject::from(array))
}
