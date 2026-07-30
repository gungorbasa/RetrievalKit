mod dto;
mod error;

use std::collections::BTreeMap;

use dto::{
    candidate_projection, graph_result, hybrid_hits, keyword_hits, record_chunks, search_hits,
    FilterDto, GraphQueryDto, GraphResultDto, GraphSchemaDto, HybridOptionsDto, RecordInputDto,
    SearchOptionsDto,
};
use error::{BoundaryError, Result};
use js_sys::Float32Array;
use retrievalkit_core::{
    CorpusChunkInput, CorpusId, HybridQuery, KeywordQuery, RecordInput, RetrievalDatabaseBuilder,
    SearchQuery, VectorEncoding, VectorMetric,
};
use retrievalkit_graph::{
    GraphDatabase as CoreGraphDatabase, GraphDatabaseBuilder, GraphResult,
    GraphRetrievalDatabase as CoreGraphRetrievalDatabase, GraphRetrievalDatabaseBuilder,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildCapabilities {
    persistence: bool,
    threads: bool,
    simd: bool,
    execution: &'static str,
    performance_tier: &'static str,
    structured_dtos: bool,
    bulk_float32_embeddings: bool,
}

/// Compile-time browser capabilities. Persistence and threaded WASM are
/// intentionally excluded from this first in-memory artifact.
#[wasm_bindgen(js_name = buildCapabilities)]
pub fn build_capabilities() -> std::result::Result<JsValue, JsError> {
    let simd = cfg!(all(target_arch = "wasm32", feature = "wasm-simd128"));
    to_js(&BuildCapabilities {
        persistence: false,
        threads: false,
        simd,
        execution: "dedicated-worker",
        performance_tier: if simd { "simd128" } else { "portable" },
        structured_dtos: true,
        bulk_float32_embeddings: true,
    })
}

enum RetrievalState {
    Building(RetrievalDatabaseBuilder),
    Ready(retrievalkit_core::RetrievalDatabase),
    Closed,
}

#[wasm_bindgen(js_name = RetrievalDatabase)]
pub struct WasmRetrievalDatabase {
    state: RetrievalState,
}

#[wasm_bindgen(js_class = RetrievalDatabase)]
impl WasmRetrievalDatabase {
    #[wasm_bindgen(constructor)]
    pub fn new(
        corpus_id: String,
        metric: String,
        encoding: String,
    ) -> std::result::Result<Self, JsError> {
        Ok(Self {
            state: RetrievalState::Building(RetrievalDatabaseBuilder::new(
                CorpusId::new(corpus_id).map_err(BoundaryError::core)?,
                parse_metric(&metric)?,
                parse_encoding(&encoding)?,
            )),
        })
    }

    /// Adds records with one flattened row-major Float32Array. Each chunk's
    /// `embeddingIndex` selects a row; no per-vector JS/WASM calls occur.
    #[wasm_bindgen(js_name = addRecordsBatch)]
    pub fn add_records_batch(
        &mut self,
        records: JsValue,
        embeddings: Float32Array,
        dimension: u32,
    ) -> std::result::Result<u32, JsError> {
        let records: Vec<RecordInputDto> = from_js(records)?;
        let vectors = validate_vectors(embeddings, dimension)?;
        let builder = match &mut self.state {
            RetrievalState::Building(builder) => builder,
            RetrievalState::Ready(_) => {
                return Err(BoundaryError::state("retrieval database", "building").into())
            }
            RetrievalState::Closed => {
                return Err(BoundaryError::state("retrieval database", "open").into())
            }
        };
        let mut inserted = 0usize;
        for input in records {
            let (record, metadata, chunks) = input.into_record()?;
            inserted += builder
                .upsert_record(
                    record,
                    metadata,
                    record_chunks(chunks, &vectors, dimension as usize)?,
                )
                .map_err(BoundaryError::core)?
                .len();
        }
        count_u32(inserted, "inserted chunk count")
    }

    pub fn build(&mut self) -> std::result::Result<(), JsError> {
        let state = std::mem::replace(&mut self.state, RetrievalState::Closed);
        self.state = match state {
            RetrievalState::Building(builder) => {
                RetrievalState::Ready(builder.build().map_err(BoundaryError::core)?)
            }
            RetrievalState::Ready(database) => {
                self.state = RetrievalState::Ready(database);
                return Err(BoundaryError::state("retrieval database", "building").into());
            }
            RetrievalState::Closed => {
                return Err(BoundaryError::state("retrieval database", "open").into())
            }
        };
        Ok(())
    }

    #[wasm_bindgen(js_name = vectorSearch)]
    pub fn vector_search(
        &self,
        embedding: Float32Array,
        options: JsValue,
    ) -> std::result::Result<JsValue, JsError> {
        let options: SearchOptionsDto = from_js(options)?;
        let database = self.ready()?;
        let mut query = SearchQuery::new(embedding.to_vec(), options.top_k);
        if let Some(filter) = options.filter {
            query = query.with_filter(filter.into_core()?);
        }
        let hits = database
            .semantic_search(&query)
            .map_err(BoundaryError::core)?;
        to_js(&search_hits(hits, |id| database.chunk(id).cloned()))
    }

    #[wasm_bindgen(js_name = bm25Search)]
    pub fn bm25_search(
        &self,
        text: String,
        options: JsValue,
    ) -> std::result::Result<JsValue, JsError> {
        let options: SearchOptionsDto = from_js(options)?;
        let database = self.ready()?;
        let mut query = KeywordQuery::new(text, options.top_k);
        if let Some(filter) = options.filter {
            query = query.with_filter(filter.into_core()?);
        }
        let hits = database
            .keyword_search(&query)
            .map_err(BoundaryError::core)?;
        to_js(&keyword_hits(hits, |id| database.chunk(id).cloned()))
    }

    #[wasm_bindgen(js_name = hybridSearch)]
    pub fn hybrid_search(
        &self,
        embedding: Float32Array,
        options: JsValue,
    ) -> std::result::Result<JsValue, JsError> {
        let options: HybridOptionsDto = from_js(options)?;
        let database = self.ready()?;
        let mut query = HybridQuery::new(options.text, embedding.to_vec(), options.top_k)
            .try_with_alpha(options.alpha)
            .map_err(BoundaryError::core)?;
        if options.vector_candidates.is_some() || options.keyword_candidates.is_some() {
            query = query.with_candidate_limits(
                options.vector_candidates.unwrap_or(50),
                options.keyword_candidates.unwrap_or(50),
            );
        }
        if let Some(filter) = options.filter {
            query = query.with_filter(filter.into_core()?);
        }
        let hits = database
            .hybrid_search(&query)
            .map_err(BoundaryError::core)?;
        to_js(&hybrid_hits(hits, |id| database.chunk(id).cloned()))
    }

    pub fn close(&mut self) {
        self.state = RetrievalState::Closed;
    }
}

impl WasmRetrievalDatabase {
    fn ready(&self) -> Result<&retrievalkit_core::RetrievalDatabase> {
        match &self.state {
            RetrievalState::Ready(database) => Ok(database),
            RetrievalState::Building(_) => Err(BoundaryError::state("retrieval database", "built")),
            RetrievalState::Closed => Err(BoundaryError::state("retrieval database", "open")),
        }
    }
}

enum GraphState {
    Building(Box<GraphDatabaseBuilder>),
    Ready(Box<CoreGraphDatabase>),
    Closed,
}

#[wasm_bindgen(js_name = GraphDatabase)]
pub struct WasmGraphDatabase {
    state: GraphState,
    selections: BTreeMap<u32, GraphResult>,
    next_selection_id: u32,
}

#[wasm_bindgen(js_class = GraphDatabase)]
impl WasmGraphDatabase {
    #[wasm_bindgen(constructor)]
    pub fn new(corpus_id: String, schema: JsValue) -> std::result::Result<Self, JsError> {
        let schema: GraphSchemaDto = from_js(schema)?;
        Ok(Self {
            state: GraphState::Building(Box::new(GraphDatabaseBuilder::new(
                CorpusId::new(corpus_id).map_err(BoundaryError::core)?,
                schema.into_core()?,
            ))),
            selections: BTreeMap::new(),
            next_selection_id: 0,
        })
    }

    #[wasm_bindgen(js_name = addRecordsBatch)]
    pub fn add_records_batch(&mut self, records: JsValue) -> std::result::Result<u32, JsError> {
        let records: Vec<RecordInputDto> = from_js(records)?;
        let builder = match &mut self.state {
            GraphState::Building(builder) => builder,
            GraphState::Ready(_) => {
                return Err(BoundaryError::state("graph database", "building").into())
            }
            GraphState::Closed => return Err(BoundaryError::state("graph database", "open").into()),
        };
        let count = records.len();
        for input in records {
            let (record, metadata, chunks) = input.into_record()?;
            builder
                .upsert_input(RecordInput {
                    record,
                    metadata,
                    chunks: chunks
                        .into_iter()
                        .map(|chunk| {
                            Ok(CorpusChunkInput {
                                key: retrievalkit_core::ChunkKey::new(chunk.key)
                                    .map_err(BoundaryError::core)?,
                                text: chunk.text,
                                metadata: dto::metadata_from_dto(chunk.metadata)?,
                            })
                        })
                        .collect::<Result<_>>()?,
                })
                .map_err(BoundaryError::graph)?;
        }
        count_u32(count, "record count")
    }

    pub fn build(&mut self) -> std::result::Result<(), JsError> {
        let state = std::mem::replace(&mut self.state, GraphState::Closed);
        self.state = match state {
            GraphState::Building(builder) => {
                GraphState::Ready(Box::new((*builder).build().map_err(BoundaryError::graph)?))
            }
            GraphState::Ready(database) => {
                self.state = GraphState::Ready(database);
                return Err(BoundaryError::state("graph database", "building").into());
            }
            GraphState::Closed => return Err(BoundaryError::state("graph database", "open").into()),
        };
        Ok(())
    }

    pub fn query(&mut self, query: JsValue) -> std::result::Result<JsValue, JsError> {
        let query: GraphQueryDto = from_js(query)?;
        let result = self
            .ready()?
            .graph_query(&query.into_core()?, None)
            .map_err(BoundaryError::graph)?;
        self.store_selection(result)
    }

    #[wasm_bindgen(js_name = projectCandidates)]
    pub fn project_candidates(
        &self,
        selection_id: u32,
        filter: JsValue,
    ) -> std::result::Result<JsValue, JsError> {
        let filter = optional_filter(filter)?;
        let projection = self
            .ready()?
            .project_candidate_identities(self.selection(selection_id)?, filter.as_ref())
            .map_err(BoundaryError::graph)?;
        to_js(&candidate_projection(projection))
    }

    #[wasm_bindgen(js_name = releaseSelection)]
    pub fn release_selection(&mut self, selection_id: u32) -> bool {
        self.selections.remove(&selection_id).is_some()
    }

    pub fn close(&mut self) {
        self.state = GraphState::Closed;
        self.selections.clear();
    }
}

impl WasmGraphDatabase {
    fn ready(&self) -> Result<&CoreGraphDatabase> {
        match &self.state {
            GraphState::Ready(database) => Ok(database),
            GraphState::Building(_) => Err(BoundaryError::state("graph database", "built")),
            GraphState::Closed => Err(BoundaryError::state("graph database", "open")),
        }
    }

    fn selection(&self, id: u32) -> Result<&GraphResult> {
        self.selections.get(&id).ok_or_else(|| {
            BoundaryError::invalid(
                "selectionId",
                format!("unknown or released graph selection {id}"),
            )
        })
    }

    fn store_selection(&mut self, result: GraphResult) -> std::result::Result<JsValue, JsError> {
        let selection_id = self.next_selection_id;
        self.next_selection_id = self
            .next_selection_id
            .checked_add(1)
            .ok_or_else(|| BoundaryError::invalid("selectionId", "selection counter exhausted"))?;
        let dto = graph_result(selection_id, &result);
        self.selections.insert(selection_id, result);
        to_js(&dto)
    }
}

enum GraphRetrievalState {
    Building(Box<GraphRetrievalDatabaseBuilder>),
    Ready(Box<CoreGraphRetrievalDatabase>),
    Closed,
}

#[wasm_bindgen(js_name = GraphRetrievalDatabase)]
pub struct WasmGraphRetrievalDatabase {
    state: GraphRetrievalState,
    selections: BTreeMap<u32, GraphResult>,
    next_selection_id: u32,
}

#[wasm_bindgen(js_class = GraphRetrievalDatabase)]
impl WasmGraphRetrievalDatabase {
    #[wasm_bindgen(constructor)]
    pub fn new(
        corpus_id: String,
        schema: JsValue,
        metric: String,
        encoding: String,
    ) -> std::result::Result<Self, JsError> {
        let schema: GraphSchemaDto = from_js(schema)?;
        Ok(Self {
            state: GraphRetrievalState::Building(Box::new(GraphRetrievalDatabaseBuilder::new(
                CorpusId::new(corpus_id).map_err(BoundaryError::core)?,
                schema.into_core()?,
                parse_metric(&metric)?,
                parse_encoding(&encoding)?,
            ))),
            selections: BTreeMap::new(),
            next_selection_id: 0,
        })
    }

    #[wasm_bindgen(js_name = addRecordsBatch)]
    pub fn add_records_batch(
        &mut self,
        records: JsValue,
        embeddings: Float32Array,
        dimension: u32,
    ) -> std::result::Result<u32, JsError> {
        let records: Vec<RecordInputDto> = from_js(records)?;
        let vectors = validate_vectors(embeddings, dimension)?;
        let builder = match &mut self.state {
            GraphRetrievalState::Building(builder) => builder,
            GraphRetrievalState::Ready(_) => {
                return Err(BoundaryError::state("graph retrieval database", "building").into())
            }
            GraphRetrievalState::Closed => {
                return Err(BoundaryError::state("graph retrieval database", "open").into())
            }
        };
        let mut inserted = 0usize;
        for input in records {
            let (record, metadata, chunks) = input.into_record()?;
            inserted += builder
                .upsert_record_chunks(
                    record,
                    metadata,
                    record_chunks(chunks, &vectors, dimension as usize)?,
                )
                .map_err(BoundaryError::graph)?
                .len();
        }
        count_u32(inserted, "inserted chunk count")
    }

    pub fn build(&mut self) -> std::result::Result<(), JsError> {
        let state = std::mem::replace(&mut self.state, GraphRetrievalState::Closed);
        self.state = match state {
            GraphRetrievalState::Building(builder) => GraphRetrievalState::Ready(Box::new(
                (*builder).build().map_err(BoundaryError::graph)?,
            )),
            GraphRetrievalState::Ready(database) => {
                self.state = GraphRetrievalState::Ready(database);
                return Err(BoundaryError::state("graph retrieval database", "building").into());
            }
            GraphRetrievalState::Closed => {
                return Err(BoundaryError::state("graph retrieval database", "open").into())
            }
        };
        Ok(())
    }

    #[wasm_bindgen(js_name = graphQuery)]
    pub fn graph_query(&mut self, query: JsValue) -> std::result::Result<JsValue, JsError> {
        let query: GraphQueryDto = from_js(query)?;
        let result = self
            .ready()?
            .graph_query(&query.into_core()?, None)
            .map_err(BoundaryError::graph)?;
        self.store_selection(result)
    }

    #[wasm_bindgen(js_name = projectCandidates)]
    pub fn project_candidates(
        &self,
        selection_id: u32,
        filter: JsValue,
    ) -> std::result::Result<JsValue, JsError> {
        let filter = optional_filter(filter)?;
        let projection = self
            .ready()?
            .project_candidate_identities(self.selection(selection_id)?, filter.as_ref())
            .map_err(BoundaryError::graph)?;
        to_js(&candidate_projection(projection))
    }

    #[wasm_bindgen(js_name = vectorSearch)]
    pub fn vector_search(
        &self,
        embedding: Float32Array,
        options: JsValue,
        selection_id: Option<u32>,
    ) -> std::result::Result<JsValue, JsError> {
        let options: SearchOptionsDto = from_js(options)?;
        let database = self.ready()?;
        let mut query = SearchQuery::new(embedding.to_vec(), options.top_k);
        if let Some(filter) = options.filter {
            query = query.with_filter(filter.into_core()?);
        }
        let hits = match selection_id {
            Some(id) => database
                .semantic_search_in_selection(&query, self.selection(id)?)
                .map_err(BoundaryError::graph)?,
            None => database
                .semantic_search(&query)
                .map_err(BoundaryError::graph)?,
        };
        to_js(&search_hits(hits, |id| {
            database.retrieval().chunk(id).cloned()
        }))
    }

    #[wasm_bindgen(js_name = bm25Search)]
    pub fn bm25_search(
        &self,
        text: String,
        options: JsValue,
        selection_id: Option<u32>,
    ) -> std::result::Result<JsValue, JsError> {
        let options: SearchOptionsDto = from_js(options)?;
        let database = self.ready()?;
        let mut query = KeywordQuery::new(text, options.top_k);
        if let Some(filter) = options.filter {
            query = query.with_filter(filter.into_core()?);
        }
        let hits = match selection_id {
            Some(id) => database
                .keyword_search_in_selection(&query, self.selection(id)?)
                .map_err(BoundaryError::graph)?,
            None => database
                .keyword_search(&query)
                .map_err(BoundaryError::graph)?,
        };
        to_js(&keyword_hits(hits, |id| {
            database.retrieval().chunk(id).cloned()
        }))
    }

    #[wasm_bindgen(js_name = hybridSearch)]
    pub fn hybrid_search(
        &self,
        embedding: Float32Array,
        options: JsValue,
        selection_id: Option<u32>,
    ) -> std::result::Result<JsValue, JsError> {
        let options: HybridOptionsDto = from_js(options)?;
        let database = self.ready()?;
        let mut query = HybridQuery::new(options.text, embedding.to_vec(), options.top_k)
            .try_with_alpha(options.alpha)
            .map_err(BoundaryError::core)?;
        if options.vector_candidates.is_some() || options.keyword_candidates.is_some() {
            query = query.with_candidate_limits(
                options.vector_candidates.unwrap_or(50),
                options.keyword_candidates.unwrap_or(50),
            );
        }
        if let Some(filter) = options.filter {
            query = query.with_filter(filter.into_core()?);
        }
        let hits = match selection_id {
            Some(id) => database
                .hybrid_search_in_selection(&query, self.selection(id)?)
                .map_err(BoundaryError::graph)?,
            None => database
                .hybrid_search(&query)
                .map_err(BoundaryError::graph)?,
        };
        to_js(&hybrid_hits(hits, |id| {
            database.retrieval().chunk(id).cloned()
        }))
    }

    #[wasm_bindgen(js_name = releaseSelection)]
    pub fn release_selection(&mut self, selection_id: u32) -> bool {
        self.selections.remove(&selection_id).is_some()
    }

    pub fn close(&mut self) {
        self.state = GraphRetrievalState::Closed;
        self.selections.clear();
    }
}

impl WasmGraphRetrievalDatabase {
    fn ready(&self) -> Result<&CoreGraphRetrievalDatabase> {
        match &self.state {
            GraphRetrievalState::Ready(database) => Ok(database),
            GraphRetrievalState::Building(_) => {
                Err(BoundaryError::state("graph retrieval database", "built"))
            }
            GraphRetrievalState::Closed => {
                Err(BoundaryError::state("graph retrieval database", "open"))
            }
        }
    }

    fn selection(&self, id: u32) -> Result<&GraphResult> {
        self.selections.get(&id).ok_or_else(|| {
            BoundaryError::invalid(
                "selectionId",
                format!("unknown or released graph selection {id}"),
            )
        })
    }

    fn store_selection(&mut self, result: GraphResult) -> std::result::Result<JsValue, JsError> {
        let selection_id = self.next_selection_id;
        self.next_selection_id = self
            .next_selection_id
            .checked_add(1)
            .ok_or_else(|| BoundaryError::invalid("selectionId", "selection counter exhausted"))?;
        let dto: GraphResultDto = graph_result(selection_id, &result);
        self.selections.insert(selection_id, result);
        to_js(&dto)
    }
}

fn parse_metric(value: &str) -> Result<VectorMetric> {
    match value {
        "dotProduct" => Ok(VectorMetric::DotProduct),
        "cosine" => Ok(VectorMetric::Cosine),
        actual => Err(BoundaryError::invalid(
            "metric",
            format!("expected 'dotProduct' or 'cosine', got '{actual}'"),
        )),
    }
}

fn parse_encoding(value: &str) -> Result<VectorEncoding> {
    match value {
        "f32" => Ok(VectorEncoding::F32),
        "f16" => Ok(VectorEncoding::F16),
        "bf16" => Ok(VectorEncoding::BF16),
        "i8" => Ok(VectorEncoding::I8ScalarQuantized),
        "binary" => Ok(VectorEncoding::BinaryQuantized),
        actual => Err(BoundaryError::invalid(
            "encoding",
            format!("expected f32, f16, bf16, i8, or binary; got '{actual}'"),
        )),
    }
}

fn validate_vectors(values: Float32Array, dimension: u32) -> Result<Vec<f32>> {
    if dimension == 0 {
        return Err(BoundaryError::invalid(
            "dimension",
            "must be greater than zero",
        ));
    }
    let vectors = values.to_vec();
    if !vectors.len().is_multiple_of(dimension as usize) {
        return Err(BoundaryError::invalid(
            "embeddings",
            format!(
                "Float32Array length {} is not divisible by dimension {dimension}",
                vectors.len()
            ),
        ));
    }
    Ok(vectors)
}

fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T> {
    serde_wasm_bindgen::from_value(value).map_err(BoundaryError::serde)
}

fn optional_filter(value: JsValue) -> Result<Option<retrievalkit_core::Filter>> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let filter: FilterDto = from_js(value)?;
    filter.into_core().map(Some)
}

fn to_js<T: Serialize>(value: &T) -> std::result::Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value)
        .map_err(BoundaryError::serde)
        .map_err(Into::into)
}

fn count_u32(value: usize, path: &str) -> std::result::Result<u32, JsError> {
    value
        .try_into()
        .map_err(|_| BoundaryError::invalid(path, "exceeds the JavaScript boundary limit").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dto::{GraphSeedDto, NodeIdDto, NodeSourceKindDto, RecordNodeSchemaDto};
    use retrievalkit_core::{Metadata, Record, RecordId, RecordType};

    #[test]
    fn metric_and_encoding_names_are_explicit() {
        assert_eq!(parse_metric("cosine").unwrap(), VectorMetric::Cosine);
        assert_eq!(parse_encoding("f32").unwrap(), VectorEncoding::F32);
        assert!(parse_metric("innerProduct").is_err());
    }

    #[test]
    fn build_and_search_uses_the_existing_core_without_persistence() {
        let mut builder = RetrievalDatabaseBuilder::new(
            CorpusId::new("wasm-test").unwrap(),
            VectorMetric::DotProduct,
            VectorEncoding::F32,
        );
        let input = RecordInputDto {
            id: "one".to_owned(),
            record_type: "Topic".to_owned(),
            fields: Vec::new(),
            content: Some("fast browser retrieval".to_owned()),
            metadata: Vec::new(),
            chunks: vec![dto::ChunkInputDto {
                key: "summary".to_owned(),
                text: "fast browser retrieval".to_owned(),
                metadata: Vec::new(),
                embedding_index: Some(0),
            }],
        };
        let (record, metadata, chunks) = input.into_record().unwrap();
        builder
            .upsert_record(
                record,
                metadata,
                record_chunks(chunks, &[1.0, 0.0], 2).unwrap(),
            )
            .unwrap();
        let database = builder.build().unwrap();
        let hits = database
            .semantic_search(&SearchQuery::new(vec![1.0, 0.0], 1))
            .unwrap();
        assert_eq!(hits[0].document_id, "one");
    }

    #[test]
    fn graph_dtos_build_and_query_the_graph_only_product() {
        let schema = GraphSchemaDto {
            record_nodes: vec![RecordNodeSchemaDto {
                record_type: "Topic".to_owned(),
                node_type: "Topic".to_owned(),
                queryable_fields: Vec::new(),
            }],
            relationships: Vec::new(),
            chunk_nodes: None,
        }
        .into_core()
        .unwrap();
        let mut builder = GraphDatabaseBuilder::new(CorpusId::new("wasm-graph").unwrap(), schema);
        builder
            .upsert_record(
                Record {
                    id: RecordId::new("one").unwrap(),
                    record_type: RecordType::new("Topic").unwrap(),
                    fields: BTreeMap::new(),
                    content: None,
                },
                Metadata::new(),
            )
            .unwrap();
        let database = builder.build().unwrap();
        let query = GraphQueryDto {
            seed: GraphSeedDto::NodeIds {
                nodes: vec![NodeIdDto {
                    node_type: "Topic".to_owned(),
                    source_kind: NodeSourceKindDto::Record,
                    record_id: "one".to_owned(),
                    chunk_key: None,
                }],
            },
            steps: Vec::new(),
            limits: None,
        }
        .into_core()
        .unwrap();
        let result = database.graph_query(&query, None).unwrap();
        assert_eq!(result.matches.len(), 1);
    }

    #[test]
    fn closed_handles_reject_follow_up_operations() {
        let retrieval = WasmRetrievalDatabase {
            state: RetrievalState::Closed,
        };
        assert!(retrieval.ready().is_err());

        let graph = WasmGraphDatabase {
            state: GraphState::Closed,
            selections: BTreeMap::new(),
            next_selection_id: 0,
        };
        assert!(graph.ready().is_err());
        assert!(graph.selection(0).is_err());
    }
}
