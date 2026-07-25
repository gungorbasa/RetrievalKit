use std::collections::BTreeMap;

use jni::objects::{JClass, JFloatArray, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jint, jlong, JNI_FALSE};
use jni::JNIEnv;
use retrievalkit_core::{FieldName, Metadata, Record, RecordId, RecordType, RecordValue};
use retrievalkit_graph::{
    Cardinality, ChunkNodeSchema, Direction, DuplicateReferencePolicy, FieldPath, GraphDatabase,
    GraphDatabaseBuilder, GraphError, GraphQuery, GraphResult, GraphRetrievalDatabase,
    GraphRetrievalDatabaseBuilder, GraphScalar, GraphSchema, MissingTargetPolicy, NodeId,
    NodeSource, NodeType, QueryLimits, RecordNodeSchema, RelationshipSchema, RelationshipType,
    Seed, Traverse, TruncationReason,
};

use crate::base::{
    filter, float_array, insert_resource, java_list, lock_resource, metadata, metadata_value,
    method_int, method_object, remove_resource, resource, string, string_array, vector_encoding,
    vector_metric, with_env, with_env_object, BoundaryError, BoundaryResult, Resource,
};

impl From<GraphError> for BoundaryError {
    fn from(error: GraphError) -> Self {
        let class = match error {
            GraphError::InvalidSchema { .. } => "ai/retrievalkit/InvalidGraphSchemaException",
            GraphError::InvalidRecord { .. } => "ai/retrievalkit/InvalidGraphRecordException",
            GraphError::InvalidQuery { .. } => "ai/retrievalkit/InvalidGraphQueryException",
            GraphError::InvalidDimension { .. } => "ai/retrievalkit/InvalidDimensionException",
            GraphError::MissingEmbedding { .. } => "ai/retrievalkit/MissingEmbeddingException",
            GraphError::StaleGeneration { .. } => "ai/retrievalkit/StaleSelectionException",
            GraphError::QueryLimitExceeded { .. } => "ai/retrievalkit/GraphLimitException",
            GraphError::InvalidSnapshot { .. }
            | GraphError::IncompatibleVersion { .. }
            | GraphError::GraphUnavailable { .. }
            | GraphError::WriterBusy { .. } => "ai/retrievalkit/GraphPersistenceException",
            GraphError::MissingTarget { .. } => "ai/retrievalkit/InvalidGraphRecordException",
            GraphError::Cancelled | GraphError::TimedOut { .. } => {
                "ai/retrievalkit/InvalidGraphQueryException"
            }
            GraphError::Core { .. } => "ai/retrievalkit/RetrievalKitException",
        };
        Self {
            class,
            message: error.to_string(),
        }
    }
}

fn object_map<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<Vec<(JObject<'local>, JObject<'local>)>> {
    let entries = method_object(env, object, "entrySet", "()Ljava/util/Set;")?;
    let iterator = method_object(env, &entries, "iterator", "()Ljava/util/Iterator;")?;
    let mut values = Vec::new();
    loop {
        let has_next = env.call_method(&iterator, "hasNext", "()Z", &[])?.z()?;
        if !has_next {
            break;
        }
        let entry = env
            .call_method(&iterator, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        values.push((
            method_object(env, &entry, "getKey", "()Ljava/lang/Object;")?,
            method_object(env, &entry, "getValue", "()Ljava/lang/Object;")?,
        ));
    }
    Ok(values)
}

fn public_metadata<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<Metadata> {
    let mut metadata = BTreeMap::new();
    for (key, value) in object_map(env, object)? {
        metadata.insert(string(env, &key)?, metadata_value(env, &value)?);
    }
    Ok(metadata)
}

fn record_value<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<RecordValue> {
    if env.is_instance_of(object, "ai/retrievalkit/RecordValue$Null")? {
        Ok(RecordValue::Null)
    } else if env.is_instance_of(object, "ai/retrievalkit/RecordValue$Text")? {
        let value = method_object(env, object, "getValue", "()Ljava/lang/String;")?;
        Ok(RecordValue::String(string(env, &value)?))
    } else if env.is_instance_of(object, "ai/retrievalkit/RecordValue$Integer")? {
        Ok(RecordValue::I64(
            env.call_method(object, "getValue", "()J", &[])?.j()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/RecordValue$Decimal")? {
        Ok(RecordValue::F64(
            env.call_method(object, "getValue", "()D", &[])?.d()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/RecordValue$Boolean")? {
        Ok(RecordValue::Bool(
            env.call_method(object, "getValue", "()Z", &[])?.z()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/RecordValue$ListValue")? {
        let values = method_object(env, object, "getValues", "()Ljava/util/List;")?;
        Ok(RecordValue::List(
            java_list(env, &values)?
                .iter()
                .map(|value| record_value(env, value))
                .collect::<BoundaryResult<Vec<_>>>()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/RecordValue$ObjectValue")? {
        let values = method_object(env, object, "getValues", "()Ljava/util/Map;")?;
        let mut parsed = BTreeMap::new();
        for (key, value) in object_map(env, &values)? {
            parsed.insert(
                FieldName::new(string(env, &key)?)?,
                record_value(env, &value)?,
            );
        }
        Ok(RecordValue::Map(parsed))
    } else {
        Err(BoundaryError::invalid(
            "record field has an unsupported Kotlin RecordValue subtype",
        ))
    }
}

fn record<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<(Record, Metadata)> {
    let id = method_object(env, object, "getId", "()Ljava/lang/String;")?;
    let record_type = method_object(env, object, "getType", "()Ljava/lang/String;")?;
    let fields = method_object(env, object, "getFields", "()Ljava/util/Map;")?;
    let content = method_object(env, object, "getContent", "()Ljava/lang/String;")?;
    let projected_metadata = method_object(env, object, "getMetadata", "()Ljava/util/Map;")?;
    let mut parsed_fields = BTreeMap::new();
    for (key, value) in object_map(env, &fields)? {
        parsed_fields.insert(
            FieldName::new(string(env, &key)?)?,
            record_value(env, &value)?,
        );
    }
    Ok((
        Record {
            id: RecordId::new(string(env, &id)?)?,
            record_type: RecordType::new(string(env, &record_type)?)?,
            fields: parsed_fields,
            content: (!content.is_null())
                .then(|| string(env, &content))
                .transpose()?,
        },
        public_metadata(env, &projected_metadata)?,
    ))
}

fn field_path<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<FieldPath> {
    let segments = method_object(env, object, "getSegments", "()Ljava/util/List;")?;
    FieldPath::new(
        java_list(env, &segments)?
            .iter()
            .map(|segment| Ok(FieldName::new(string(env, segment)?)?))
            .collect::<BoundaryResult<Vec<_>>>()?,
    )
    .map_err(Into::into)
}

fn graph_schema<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<GraphSchema> {
    let record_nodes = method_object(env, object, "getRecordNodes", "()Ljava/util/List;")?;
    let mut parsed_nodes = Vec::new();
    for node in java_list(env, &record_nodes)? {
        let record_type = method_object(env, &node, "getRecordType", "()Ljava/lang/String;")?;
        let node_type = method_object(env, &node, "getNodeType", "()Ljava/lang/String;")?;
        let fields = method_object(env, &node, "getQueryableFields", "()Ljava/util/List;")?;
        parsed_nodes.push(RecordNodeSchema {
            record_type: RecordType::new(string(env, &record_type)?)?,
            node_type: NodeType::new(string(env, &node_type)?)?,
            queryable_fields: java_list(env, &fields)?
                .iter()
                .map(|field| field_path(env, field))
                .collect::<BoundaryResult<Vec<_>>>()?,
        });
    }
    let relationships = method_object(env, object, "getRelationships", "()Ljava/util/List;")?;
    let mut parsed_relationships = Vec::new();
    for relationship in java_list(env, &relationships)? {
        let relationship_type = method_object(
            env,
            &relationship,
            "getRelationshipType",
            "()Ljava/lang/String;",
        )?;
        let source_node = method_object(
            env,
            &relationship,
            "getSourceNodeType",
            "()Ljava/lang/String;",
        )?;
        let target_node = method_object(
            env,
            &relationship,
            "getTargetNodeType",
            "()Ljava/lang/String;",
        )?;
        let source_field = method_object(
            env,
            &relationship,
            "getSourceField",
            "()Lai/retrievalkit/FieldPath;",
        )?;
        let cardinality = method_object(
            env,
            &relationship,
            "getCardinality",
            "()Lai/retrievalkit/Cardinality;",
        )?;
        let missing = method_object(
            env,
            &relationship,
            "getMissingTarget",
            "()Lai/retrievalkit/MissingTargetPolicy;",
        )?;
        let duplicates = method_object(
            env,
            &relationship,
            "getDuplicateReferences",
            "()Lai/retrievalkit/DuplicateReferencePolicy;",
        )?;
        let inverse = method_object(
            env,
            &relationship,
            "getInverseRelationship",
            "()Ljava/lang/String;",
        )?;
        parsed_relationships.push(RelationshipSchema {
            relationship_type: RelationshipType::new(string(env, &relationship_type)?)?,
            source_node_type: NodeType::new(string(env, &source_node)?)?,
            target_node_type: NodeType::new(string(env, &target_node)?)?,
            source_field: field_path(env, &source_field)?,
            cardinality: match method_int(env, &cardinality, "ordinal")? {
                0 => Cardinality::One,
                1 => Cardinality::OptionalOne,
                2 => Cardinality::Many,
                value => {
                    return Err(BoundaryError::invalid(format!(
                        "cardinality ordinal {value} is unsupported"
                    )))
                }
            },
            missing_target: match method_int(env, &missing, "ordinal")? {
                0 => MissingTargetPolicy::Error,
                1 => MissingTargetPolicy::OmitEdge,
                value => {
                    return Err(BoundaryError::invalid(format!(
                        "missing-target ordinal {value} is unsupported"
                    )))
                }
            },
            duplicate_references: match method_int(env, &duplicates, "ordinal")? {
                0 => DuplicateReferencePolicy::Error,
                1 => DuplicateReferencePolicy::Deduplicate,
                value => {
                    return Err(BoundaryError::invalid(format!(
                        "duplicate-reference ordinal {value} is unsupported"
                    )))
                }
            },
            allow_self_edge: env
                .call_method(&relationship, "getAllowSelfEdge", "()Z", &[])?
                .z()?,
            inverse_relationship: if inverse.is_null() {
                None
            } else {
                Some(RelationshipType::new(string(env, &inverse)?)?)
            },
        });
    }
    let chunk = method_object(
        env,
        object,
        "getChunkNodes",
        "()Lai/retrievalkit/ChunkNodeSchema;",
    )?;
    let mut schema = GraphSchema::new(parsed_nodes).with_relationships(parsed_relationships);
    if !chunk.is_null() {
        let node_type = method_object(env, &chunk, "getNodeType", "()Ljava/lang/String;")?;
        let owns = method_object(env, &chunk, "getOwnsRelationship", "()Ljava/lang/String;")?;
        let inverse = method_object(
            env,
            &chunk,
            "getInverseRelationship",
            "()Ljava/lang/String;",
        )?;
        schema = schema.with_chunk_nodes(ChunkNodeSchema {
            node_type: NodeType::new(string(env, &node_type)?)?,
            owns_relationship: RelationshipType::new(string(env, &owns)?)?,
            inverse_relationship: if inverse.is_null() {
                None
            } else {
                Some(RelationshipType::new(string(env, &inverse)?)?)
            },
        });
    }
    schema.validate()?;
    Ok(schema)
}

fn node_id<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<NodeId> {
    let node_type = method_object(env, object, "getNodeType", "()Ljava/lang/String;")?;
    let record_id = method_object(env, object, "getRecordId", "()Ljava/lang/String;")?;
    let chunk_key = method_object(env, object, "getChunkKey", "()Ljava/lang/String;")?;
    let node_type = NodeType::new(string(env, &node_type)?)?;
    let record_id = RecordId::new(string(env, &record_id)?)?;
    Ok(if chunk_key.is_null() {
        NodeId::record(node_type, record_id)
    } else {
        NodeId::chunk(
            node_type,
            retrievalkit_core::ChunkIdentity::new(
                record_id,
                retrievalkit_core::ChunkKey::new(string(env, &chunk_key)?)?,
            ),
        )
    })
}

fn graph_scalar<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<GraphScalar> {
    if env.is_instance_of(object, "ai/retrievalkit/GraphScalar$Text")? {
        let value = method_object(env, object, "getValue", "()Ljava/lang/String;")?;
        Ok(GraphScalar::String(string(env, &value)?))
    } else if env.is_instance_of(object, "ai/retrievalkit/GraphScalar$Integer")? {
        Ok(GraphScalar::I64(
            env.call_method(object, "getValue", "()J", &[])?.j()?,
        ))
    } else if env.is_instance_of(object, "ai/retrievalkit/GraphScalar$Boolean")? {
        Ok(GraphScalar::Bool(
            env.call_method(object, "getValue", "()Z", &[])?.z()?,
        ))
    } else {
        Err(BoundaryError::invalid(
            "graph scalar has an unsupported Kotlin subtype",
        ))
    }
}

fn graph_query<'local, 'object>(
    env: &mut JNIEnv<'local>,
    object: &JObject<'object>,
) -> BoundaryResult<GraphQuery> {
    let seed_object = method_object(env, object, "getSeed", "()Lai/retrievalkit/GraphSeed;")?;
    let seed = if env.is_instance_of(&seed_object, "ai/retrievalkit/GraphSeed$Nodes")? {
        let nodes = method_object(env, &seed_object, "getNodes", "()Ljava/util/List;")?;
        Seed::NodeIds(
            java_list(env, &nodes)?
                .iter()
                .map(|node| node_id(env, node))
                .collect::<BoundaryResult<Vec<_>>>()?,
        )
    } else if env.is_instance_of(&seed_object, "ai/retrievalkit/GraphSeed$Equals")? {
        let node_type = method_object(env, &seed_object, "getNodeType", "()Ljava/lang/String;")?;
        let field = method_object(
            env,
            &seed_object,
            "getField",
            "()Lai/retrievalkit/FieldPath;",
        )?;
        let values = method_object(env, &seed_object, "getValues", "()Ljava/util/List;")?;
        Seed::Equals {
            node_type: NodeType::new(string(env, &node_type)?)?,
            field: field_path(env, &field)?,
            values: java_list(env, &values)?
                .iter()
                .map(|value| graph_scalar(env, value))
                .collect::<BoundaryResult<Vec<_>>>()?,
        }
    } else {
        return Err(BoundaryError::invalid(
            "graph seed has an unsupported Kotlin subtype",
        ));
    };
    let traversals = method_object(env, object, "getTraversals", "()Ljava/util/List;")?;
    let mut query = GraphQuery::new(seed);
    for traversal in java_list(env, &traversals)? {
        let relationship =
            method_object(env, &traversal, "getRelationship", "()Ljava/lang/String;")?;
        let direction = method_object(
            env,
            &traversal,
            "getDirection",
            "()Lai/retrievalkit/GraphDirection;",
        )?;
        let min_hops = method_int(env, &traversal, "getMinHops")?;
        let max_hops = method_int(env, &traversal, "getMaxHops")?;
        query = query.traverse(Traverse {
            relationship: RelationshipType::new(string(env, &relationship)?)?,
            direction: match method_int(env, &direction, "ordinal")? {
                0 => Direction::Outgoing,
                1 => Direction::Incoming,
                value => {
                    return Err(BoundaryError::invalid(format!(
                        "graph direction ordinal {value} is unsupported"
                    )))
                }
            },
            min_hops: usize::try_from(min_hops)
                .map_err(|_| BoundaryError::invalid("minHops cannot be negative"))?,
            max_hops: usize::try_from(max_hops)
                .map_err(|_| BoundaryError::invalid("maxHops cannot be negative"))?,
        });
    }
    let limits = method_object(
        env,
        object,
        "getLimits",
        "()Lai/retrievalkit/GraphQueryLimits;",
    )?;
    let limit = |env: &mut JNIEnv<'local>, name: &str| -> BoundaryResult<usize> {
        usize::try_from(method_int(env, &limits, name)?)
            .map_err(|_| BoundaryError::invalid(format!("{name} cannot be negative")))
    };
    Ok(query.with_limits(QueryLimits {
        max_hops: limit(env, "getMaxHops")?,
        max_visited: limit(env, "getMaxVisited")?,
        max_results: limit(env, "getMaxResults")?,
        max_working_bytes: limit(env, "getMaxWorkingBytes")?,
    }))
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_createGraphBuilder(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    corpus_id: JString<'_>,
    schema: JObject<'_>,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let corpus_id = retrievalkit_core::CorpusId::new(string(env, &JObject::from(corpus_id))?)?;
        let schema = graph_schema(env, &schema)?;
        insert_resource(Resource::GraphBuilder(Box::new(GraphDatabaseBuilder::new(
            corpus_id, schema,
        ))))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_graphBuilderUpsert(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    record_object: JObject<'_>,
) {
    with_env(&mut env, (), |env| {
        let (record, metadata) = record(env, &record_object)?;
        let resource = resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::GraphBuilder(builder) = &mut *resource else {
            return Err(BoundaryError::invalid(format!(
                "GraphDatabase.Builder native handle {handle} is closed or invalid"
            )));
        };
        builder.upsert_record(record, metadata)?;
        Ok(())
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_buildGraph(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jlong {
    with_env(&mut env, 0, |_env| {
        let resource = remove_resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::GraphBuilder(builder) = std::mem::replace(&mut *resource, Resource::Closed)
        else {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} is not a GraphDatabase.Builder"
            )));
        };
        insert_resource(Resource::Graph(Box::new((*builder).build()?)))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_createGraphRetrievalBuilder(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    corpus_id: JString<'_>,
    schema: JObject<'_>,
    metric: jint,
    encoding: jint,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let corpus_id = retrievalkit_core::CorpusId::new(string(env, &JObject::from(corpus_id))?)?;
        let schema = graph_schema(env, &schema)?;
        insert_resource(Resource::GraphRetrievalBuilder(Box::new(
            GraphRetrievalDatabaseBuilder::new(
                corpus_id,
                schema,
                vector_metric(metric)?,
                vector_encoding(encoding)?,
            ),
        )))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_graphRetrievalBuilderUpsert(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    record_object: JObject<'_>,
    embedding: JObject<'_>,
    documents: JObject<'_>,
) {
    with_env(&mut env, (), |env| {
        let (record, metadata) = record(env, &record_object)?;
        let parsed_embedding = if embedding.is_null() {
            None
        } else {
            Some(float_array(env, <&JFloatArray>::from(&embedding))?)
        };
        let parsed_documents = if documents.is_null() {
            None
        } else {
            let documents = <&JObjectArray>::from(&documents);
            let len = env.get_array_length(documents)?;
            let mut parsed = Vec::with_capacity(len as usize);
            for index in 0..len {
                let embedded = env.get_object_array_element(documents, index)?;
                let document = method_object(
                    env,
                    &embedded,
                    "getDocument",
                    "()Lai/retrievalkit/Document;",
                )?;
                let id = method_object(env, &document, "getId", "()Ljava/lang/String;")?;
                let text = method_object(env, &document, "getText", "()Ljava/lang/String;")?;
                let document_metadata =
                    method_object(env, &document, "getMetadata", "()Ljava/util/Map;")?;
                let vector = method_object(env, &embedded, "getEmbedding", "()[F")?;
                parsed.push(retrievalkit_core::EmbeddedDocument {
                    document: retrievalkit_core::Document {
                        id: string(env, &id)?,
                        text: string(env, &text)?,
                        metadata: public_metadata(env, &document_metadata)?,
                    },
                    embedding: float_array(env, <&JFloatArray>::from(&vector))?,
                });
            }
            Some(parsed)
        };
        let resource = resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::GraphRetrievalBuilder(builder) = &mut *resource else {
            return Err(BoundaryError::invalid(format!(
                "GraphRetrievalDatabase.Builder native handle {handle} is closed or invalid"
            )));
        };
        match (parsed_embedding, parsed_documents) {
            (Some(embedding), None) => {
                builder.upsert_record_with_embedding(record, metadata, embedding)?;
            }
            (None, Some(documents)) => {
                builder.upsert_record_documents(record, metadata, documents)?;
            }
            (None, None) => {
                builder.upsert_record(record, metadata)?;
            }
            (Some(_), Some(_)) => {
                return Err(BoundaryError::invalid(
                    "provide either one embedding or embedded documents, not both",
                ))
            }
        }
        Ok(())
    });
}

/// Test-only fixture bridge for canonical conformance records whose stable
/// chunk identities intentionally exercise a shape outside the common
/// progressive API. This symbol is internal to the Kotlin module.
#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_graphRetrievalBuilderUpsertFixtureChunk(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    record_object: JObject<'_>,
    chunk_key: JString<'_>,
    text: JString<'_>,
    embedding: JFloatArray<'_>,
    chunk_metadata: JObject<'_>,
) {
    with_env(&mut env, (), |env| {
        let (record, projected_metadata) = record(env, &record_object)?;
        let chunk = retrievalkit_core::RecordChunkInput {
            key: retrievalkit_core::ChunkKey::new(string(env, &JObject::from(chunk_key))?)?,
            text: string(env, &JObject::from(text))?,
            embedding: float_array(env, &embedding)?,
            metadata: metadata(env, &chunk_metadata)?,
        };
        let resource = resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::GraphRetrievalBuilder(builder) = &mut *resource else {
            return Err(BoundaryError::invalid(format!(
                "GraphRetrievalDatabase.Builder native handle {handle} is closed or invalid"
            )));
        };
        builder.upsert_record_chunks(record, projected_metadata, vec![chunk])?;
        Ok(())
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_buildGraphRetrieval(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jlong {
    with_env(&mut env, 0, |_env| {
        let resource = remove_resource(handle)?;
        let mut resource = lock_resource(&resource)?;
        let Resource::GraphRetrievalBuilder(builder) =
            std::mem::replace(&mut *resource, Resource::Closed)
        else {
            return Err(BoundaryError::invalid(format!(
                "native handle {handle} is not a GraphRetrievalDatabase.Builder"
            )));
        };
        insert_resource(Resource::GraphRetrieval(Box::new((*builder).build()?)))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_graphQuery(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    query: JObject<'_>,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let query = graph_query(env, &query)?;
        let result = {
            let resource = resource(handle)?;
            let resource = lock_resource(&resource)?;
            match &*resource {
                Resource::Graph(database) => database.graph_query(&query, None)?,
                Resource::GraphRetrieval(database) => database.graph_query(&query, None)?,
                _ => {
                    return Err(BoundaryError::invalid(format!(
                        "native handle {handle} has no graph capability"
                    )))
                }
            }
        };
        insert_resource(Resource::Selection(result))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_graphSelection<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> JObject<'local> {
    with_env_object(&mut env, |env| {
        let resource = resource(handle)?;
        let resource = lock_resource(&resource)?;
        let Resource::Selection(selection) = &*resource else {
            return Err(BoundaryError::invalid(format!(
                "GraphSelection native handle {handle} is closed or invalid"
            )));
        };
        selection_object(env, selection)
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_projectCandidates<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    selection_handle: jlong,
    filter_object: JObject<'local>,
) -> JObject<'local> {
    with_env_object(&mut env, |env| {
        let parsed_filter = filter(env, &filter_object)?;
        let selection = {
            let selection = resource(selection_handle)?;
            let selection = lock_resource(&selection)?;
            let Resource::Selection(selection) = &*selection else {
                return Err(BoundaryError::invalid(format!(
                    "GraphSelection native handle {selection_handle} is closed or invalid"
                )));
            };
            selection.clone()
        };
        let resource = resource(handle)?;
        let resource = lock_resource(&resource)?;
        let projection = match &*resource {
            Resource::Graph(database) => {
                database.project_candidate_identities(&selection, parsed_filter.as_ref())?
            }
            Resource::GraphRetrieval(database) => {
                database.project_candidate_identities(&selection, parsed_filter.as_ref())?
            }
            _ => {
                return Err(BoundaryError::invalid(format!(
                    "native handle {handle} has no graph capability"
                )))
            }
        };
        let record_ids = projection
            .candidates
            .iter()
            .map(|identity| identity.record_id.as_str().to_owned())
            .collect::<Vec<_>>();
        let chunk_keys = projection
            .candidates
            .iter()
            .map(|identity| identity.chunk_key.as_str().to_owned())
            .collect::<Vec<_>>();
        let record_ids = JObject::from(string_array(env, &record_ids)?);
        let chunk_keys = JObject::from(string_array(env, &chunk_keys)?);
        Ok(env.new_object(
            "ai/retrievalkit/internal/NativeProjection",
            "([Ljava/lang/String;[Ljava/lang/String;III)V",
            &[
                JValue::Object(&record_ids),
                JValue::Object(&chunk_keys),
                JValue::Int(projection.source_nodes as i32),
                JValue::Int(projection.projected_chunks_before_filter as i32),
                JValue::Int(projection.projected_chunks_after_filter as i32),
            ],
        )?)
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_saveGraph(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    path: JString<'_>,
    retrieval: jboolean,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let path = string(env, &JObject::from(path))?;
        let resource = resource(handle)?;
        let resource = lock_resource(&resource)?;
        let report = if retrieval == JNI_FALSE {
            let Resource::Graph(database) = &*resource else {
                return Err(BoundaryError::invalid(
                    "graph-only save requires a GraphDatabase handle",
                ));
            };
            database.save_to_dir(&path)?
        } else {
            let Resource::GraphRetrieval(database) = &*resource else {
                return Err(BoundaryError::invalid(
                    "combined save requires a GraphRetrievalDatabase handle",
                ));
            };
            database.save_to_dir(&path)?
        };
        i64::try_from(report.corpus_bytes + report.schema_bytes + report.graph_bytes)
            .map_err(|_| BoundaryError::native("graph persistence size exceeds JVM Long"))
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_loadGraph(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
    retrieval: jboolean,
) -> jlong {
    with_env(&mut env, 0, |env| {
        let path = string(env, &JObject::from(path))?;
        if retrieval == JNI_FALSE {
            insert_resource(Resource::Graph(Box::new(GraphDatabase::load_from_dir(
                path,
            )?)))
        } else {
            insert_resource(Resource::GraphRetrieval(Box::new(
                GraphRetrievalDatabase::load_from_dir(path)?,
            )))
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_retrievalkit_internal_NativeBridge_validateGraph(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
    retrieval: jboolean,
) {
    with_env(&mut env, (), |env| {
        let path = string(env, &JObject::from(path))?;
        if retrieval == JNI_FALSE {
            GraphDatabase::validate_dir(path)?;
        } else {
            GraphRetrievalDatabase::validate_dir(path)?;
        }
        Ok(())
    });
}

fn node_parts(node: &NodeId) -> (&str, &str, Option<&str>) {
    match &node.source {
        NodeSource::Record(record_id) => (node.node_type.as_str(), record_id.as_str(), None),
        NodeSource::Chunk(identity) => (
            node.node_type.as_str(),
            identity.record_id.as_str(),
            Some(identity.chunk_key.as_str()),
        ),
    }
}

fn path_objects<'local>(
    env: &mut JNIEnv<'local>,
    edges: &[retrievalkit_graph::GraphPathEdge],
) -> BoundaryResult<JObjectArray<'local>> {
    let class = env.find_class("ai/retrievalkit/internal/NativeGraphPathEdge")?;
    let array = env.new_object_array(edges.len() as i32, &class, JObject::null())?;
    for (index, edge) in edges.iter().enumerate() {
        let (source_type, source_record, source_chunk) = node_parts(&edge.edge_id.source);
        let (target_type, target_record, target_chunk) = node_parts(&edge.edge_id.target);
        let relationship = JObject::from(env.new_string(edge.edge_id.relationship_type.as_str())?);
        let source_type = JObject::from(env.new_string(source_type)?);
        let source_record = JObject::from(env.new_string(source_record)?);
        let source_chunk = match source_chunk {
            Some(value) => JObject::from(env.new_string(value)?),
            None => JObject::null(),
        };
        let target_type = JObject::from(env.new_string(target_type)?);
        let target_record = JObject::from(env.new_string(target_record)?);
        let target_chunk = match target_chunk {
            Some(value) => JObject::from(env.new_string(value)?),
            None => JObject::null(),
        };
        let provenance_record =
            JObject::from(env.new_string(edge.provenance.source_record_id.as_str())?);
        let source_field = match &edge.provenance.source_field {
            Some(path) => {
                let values = path
                    .segments()
                    .iter()
                    .map(|field| field.as_str().to_owned())
                    .collect::<Vec<_>>();
                JObject::from(string_array(env, &values)?)
            }
            None => JObject::null(),
        };
        let object = env.new_object(
            &class,
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;JJLjava/lang/String;[Ljava/lang/String;ZZ)V",
            &[
                JValue::Object(&relationship),
                JValue::Object(&source_type),
                JValue::Object(&source_record),
                JValue::Object(&source_chunk),
                JValue::Object(&target_type),
                JValue::Object(&target_record),
                JValue::Object(&target_chunk),
                JValue::Long(edge.edge_id.occurrence_ordinal.into()),
                JValue::Long(edge.provenance.schema_rule_index.into()),
                JValue::Object(&provenance_record),
                JValue::Object(&source_field),
                JValue::Bool(edge.provenance.derived_inverse.into()),
                JValue::Bool(edge.provenance.built_in.into()),
            ],
        )?;
        env.set_object_array_element(&array, index as i32, object)?;
    }
    Ok(array)
}

fn selection_object<'local>(
    env: &mut JNIEnv<'local>,
    selection: &GraphResult,
) -> BoundaryResult<JObject<'local>> {
    let match_class = env.find_class("ai/retrievalkit/internal/NativeGraphMatch")?;
    let matches = env.new_object_array(
        selection.matches.len() as i32,
        &match_class,
        JObject::null(),
    )?;
    for (index, matched) in selection.matches.iter().enumerate() {
        let (node_type, record_id, chunk_key) = node_parts(&matched.node_id);
        let node_type = JObject::from(env.new_string(node_type)?);
        let record_id = JObject::from(env.new_string(record_id)?);
        let chunk_key = match chunk_key {
            Some(value) => JObject::from(env.new_string(value)?),
            None => JObject::null(),
        };
        let path = JObject::from(path_objects(env, &matched.path)?);
        let object = env.new_object(
            &match_class,
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I[Lai/retrievalkit/internal/NativeGraphPathEdge;)V",
            &[
                JValue::Object(&node_type),
                JValue::Object(&record_id),
                JValue::Object(&chunk_key),
                JValue::Int(matched.depth as i32),
                JValue::Object(&path),
            ],
        )?;
        env.set_object_array_element(&matches, index as i32, object)?;
    }
    let matches = JObject::from(matches);
    let truncation = match selection.truncated {
        None => -1,
        Some(TruncationReason::MaxHops) => 0,
        Some(TruncationReason::MaxVisited) => 1,
        Some(TruncationReason::MaxResults) => 2,
        Some(TruncationReason::MaxWorkingBytes) => 3,
    };
    Ok(env.new_object(
        "ai/retrievalkit/internal/NativeGraphSelection",
        "([Lai/retrievalkit/internal/NativeGraphMatch;IIIIII)V",
        &[
            JValue::Object(&matches),
            JValue::Int(truncation),
            JValue::Int(selection.trace.seed_count as i32),
            JValue::Int(selection.trace.visited_states as i32),
            JValue::Int(selection.trace.traversed_edges as i32),
            JValue::Int(selection.trace.result_count as i32),
            JValue::Int(selection.trace.diagnostics as i32),
        ],
    )?)
}
