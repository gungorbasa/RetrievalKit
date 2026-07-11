use std::ffi::c_char;
use std::path::Path;

use serde::Deserialize;
use vectorkit_core::{
    ChunkKey, CorpusId, ExactVectorIndex, IndexConfig, Metadata, Record, RecordChunkInput,
};
use vectorkit_graph::{GraphIndex, GraphSchema};

use super::{
    ffi_bool, ffi_ptr, parse_encoding_code, parse_metric, read_c_string, FfiError, VkStatus,
};

pub struct VkGraphBuilder {
    core: ExactVectorIndex,
}

pub struct VkGraphIndex {
    index: GraphIndex,
}

#[derive(Deserialize)]
struct RecordBatch {
    record: Record,
    #[serde(default)]
    projected_metadata: Metadata,
    chunks: Vec<RecordChunk>,
}

#[derive(Deserialize)]
struct RecordChunk {
    key: ChunkKey,
    text: String,
    embedding: Vec<f32>,
    #[serde(default)]
    metadata: Metadata,
}

#[no_mangle]
pub extern "C" fn vectorkit_graph_ffi_abi_version() -> u32 {
    1
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_builder_new(
    dimension: usize,
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    status: *mut VkStatus,
) -> *mut VkGraphBuilder {
    ffi_ptr(status, || {
        let config = IndexConfig::new(dimension, parse_metric(metric)?)
            .with_vector_encoding(parse_encoding_code(encoding)?);
        let corpus = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let core = ExactVectorIndex::try_with_config_in_corpus(config, corpus)?;
        Ok(Box::into_raw(Box::new(VkGraphBuilder { core })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_builder_upsert_record_json(
    builder: *mut VkGraphBuilder,
    record_json: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("graph builder must not be null"))?;
        let json = unsafe { read_c_string(record_json, "record_json") }?;
        let batch: RecordBatch = serde_json::from_str(&json)
            .map_err(|error| FfiError::invalid_argument(format!("invalid record JSON: {error}")))?;
        let chunks = batch
            .chunks
            .into_iter()
            .map(|chunk| RecordChunkInput {
                key: chunk.key,
                text: chunk.text,
                embedding: chunk.embedding,
                metadata: chunk.metadata,
            })
            .collect();
        builder
            .core
            .upsert_record(batch.record, batch.projected_metadata, chunks)?;
        Ok(())
    })
}

/// Consumes `builder` whether graph construction succeeds or fails.
#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_builder_build_json(
    builder: *mut VkGraphBuilder,
    schema_json: *const c_char,
    status: *mut VkStatus,
) -> *mut VkGraphIndex {
    ffi_ptr(status, || {
        if builder.is_null() {
            return Err(FfiError::invalid_argument("graph builder must not be null"));
        }
        let builder = unsafe { Box::from_raw(builder) };
        let json = unsafe { read_c_string(schema_json, "schema_json") }?;
        let schema: GraphSchema = serde_json::from_str(&json)
            .map_err(|error| FfiError::invalid_argument(format!("invalid schema JSON: {error}")))?;
        let index = GraphIndex::build(builder.core, schema)?;
        Ok(Box::into_raw(Box::new(VkGraphIndex { index })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_builder_free(builder: *mut VkGraphBuilder) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_index_load(
    directory: *const c_char,
    status: *mut VkStatus,
) -> *mut VkGraphIndex {
    ffi_ptr(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        let index = GraphIndex::load_from_dir(Path::new(&path))?;
        Ok(Box::into_raw(Box::new(VkGraphIndex { index })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_index_save(
    index: *const VkGraphIndex,
    directory: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let index = unsafe { index.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))?;
        let path = unsafe { read_c_string(directory, "directory") }?;
        index.index.save_to_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_index_validate(
    directory: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        GraphIndex::validate_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_index_free(index: *mut VkGraphIndex) {
    if !index.is_null() {
        drop(unsafe { Box::from_raw(index) });
    }
}

impl From<vectorkit_graph::GraphError> for FfiError {
    fn from(error: vectorkit_graph::GraphError) -> Self {
        Self {
            code: super::VK_STATUS_CORE_ERROR,
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::CString;
    use std::time::{SystemTime, UNIX_EPOCH};

    use vectorkit_core::{FieldName, RecordId, RecordType, RecordValue};
    use vectorkit_graph::{GraphSchema, NodeType, RecordNodeSchema};

    use super::*;

    #[test]
    fn builder_finalization_and_composite_persistence_round_trip() {
        let corpus = CString::new("swift-generic").unwrap();
        let mut status = VkStatus {
            code: 0,
            message: std::ptr::null_mut(),
        };
        let builder = unsafe { vectorkit_graph_builder_new(2, 1, 0, corpus.as_ptr(), &mut status) };
        assert!(!builder.is_null());

        let record = Record {
            id: RecordId::new("item-1").unwrap(),
            record_type: RecordType::new("Item").unwrap(),
            fields: BTreeMap::from([(
                FieldName::new("name").unwrap(),
                RecordValue::String("Generic item".to_owned()),
            )]),
            content: None,
        };
        let batch = serde_json::json!({
            "record": record,
            "projected_metadata": {},
            "chunks": [{
                "key": "body",
                "text": "generic searchable item",
                "embedding": [1.0, 0.0],
                "metadata": {}
            }]
        });
        let batch = CString::new(serde_json::to_vec(&batch).unwrap()).unwrap();
        assert!(unsafe {
            vectorkit_graph_builder_upsert_record_json(builder, batch.as_ptr(), &mut status)
        });

        let schema = GraphSchema::new(vec![RecordNodeSchema {
            record_type: RecordType::new("Item").unwrap(),
            node_type: NodeType::new("Item").unwrap(),
            queryable_fields: vec![],
        }]);
        let schema = CString::new(serde_json::to_vec(&schema).unwrap()).unwrap();
        let index =
            unsafe { vectorkit_graph_builder_build_json(builder, schema.as_ptr(), &mut status) };
        assert!(!index.is_null());

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "vectorkit-graph-ffi-{}-{nonce}",
            std::process::id()
        ));
        let path = CString::new(directory.to_string_lossy().as_bytes()).unwrap();
        assert!(unsafe { vectorkit_graph_index_save(index, path.as_ptr(), &mut status) });
        assert!(unsafe { vectorkit_graph_index_validate(path.as_ptr(), &mut status) });
        let loaded = unsafe { vectorkit_graph_index_load(path.as_ptr(), &mut status) };
        assert!(!loaded.is_null());

        unsafe {
            vectorkit_graph_index_free(loaded);
            vectorkit_graph_index_free(index);
            super::super::vectorkit_status_clear(&mut status);
        }
        let _ = std::fs::remove_dir_all(directory);
    }
}
