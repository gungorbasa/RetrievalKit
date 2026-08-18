use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::{AsyncTask, Float32Array, Task};
use napi::{Env, Result};
use napi_derive::napi;
use retrievalkit_core::{
    Bm25Config, CorpusId, Document, HybridFusionTrace, HybridHit, HybridQuery, IndexFileSizeReport,
    KeywordHit, KeywordQuery, RetrievalDatabase, RetrievalDatabaseBuilder, SearchHit, SearchQuery,
};

use crate::common::{
    closed_error, core_error, metadata_from_native, metadata_to_native, parse_encoding,
    parse_metric, state_error, NativeFilter, NativeMetadataEntry, NativeRecordInput,
    OwnedRecordInput,
};

#[napi(object)]
pub struct NativeDocumentInput {
    pub id: String,
    pub text: String,
    pub metadata: Vec<NativeMetadataEntry>,
    pub embedding: Float32Array,
}

struct OwnedDocumentInput {
    document: Document,
    embedding: Vec<f32>,
}

impl NativeDocumentInput {
    fn into_owned(self) -> Result<OwnedDocumentInput> {
        Ok(OwnedDocumentInput {
            document: Document {
                id: self.id,
                text: self.text,
                metadata: metadata_from_native(self.metadata)?,
            },
            embedding: self.embedding.to_vec(),
        })
    }
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeSearchHit {
    pub document_id: String,
    pub text: String,
    pub metadata: Vec<NativeMetadataEntry>,
    pub score: f64,
    pub vector_score: f64,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeKeywordHit {
    pub document_id: String,
    pub text: String,
    pub metadata: Vec<NativeMetadataEntry>,
    pub score: f64,
    pub matched_terms: Vec<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeHybridTrace {
    pub alpha: f64,
    pub vector_rank: Option<u32>,
    pub keyword_rank: Option<u32>,
    pub normalized_vector_score: Option<f64>,
    pub normalized_keyword_score: Option<f64>,
    pub matched_terms: Vec<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeHybridHit {
    pub document_id: String,
    pub text: String,
    pub metadata: Vec<NativeMetadataEntry>,
    pub score: f64,
    pub vector_score: Option<f64>,
    pub keyword_score: Option<f64>,
    pub trace: NativeHybridTrace,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeFileSizeReport {
    pub manifest_bytes: f64,
    pub vectors_bytes: f64,
    pub chunks_bytes: f64,
    pub records_bytes: f64,
    pub bm25_bytes: f64,
    pub tombstones_bytes: f64,
    pub total_bytes: f64,
}

impl From<IndexFileSizeReport> for NativeFileSizeReport {
    fn from(report: IndexFileSizeReport) -> Self {
        Self {
            manifest_bytes: report.manifest_bytes as f64,
            vectors_bytes: report.vectors_bytes as f64,
            chunks_bytes: report.chunks_bytes as f64,
            records_bytes: report.records_bytes as f64,
            bm25_bytes: report.bm25_bytes as f64,
            tombstones_bytes: report.tombstones_bytes as f64,
            total_bytes: report.total_bytes() as f64,
        }
    }
}

enum RetrievalState {
    Empty,
    Building(RetrievalDatabaseBuilder),
    Ready(RetrievalDatabase),
}

struct RetrievalShared {
    state: Mutex<RetrievalState>,
    closed: AtomicBool,
}

impl RetrievalShared {
    fn require_open(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(closed_error("retrieval database"))
        } else {
            Ok(())
        }
    }
}

#[napi]
pub struct NativeRetrievalHandle {
    shared: Arc<RetrievalShared>,
}

#[napi]
impl NativeRetrievalHandle {
    #[napi(constructor)]
    pub fn new(
        corpus_id: String,
        metric: String,
        encoding: String,
        bm25_k1: f64,
        bm25_b: f64,
        stop_words: Vec<String>,
    ) -> Result<Self> {
        let corpus_id = CorpusId::new(corpus_id).map_err(core_error)?;
        let metric = parse_metric(&metric)?;
        let encoding = parse_encoding(&encoding)?;
        Ok(Self {
            shared: Arc::new(RetrievalShared {
                state: Mutex::new(RetrievalState::Building(
                    RetrievalDatabaseBuilder::new(corpus_id, metric, encoding)
                        .try_with_bm25_config(
                            Bm25Config::try_new(bm25_k1 as f32, bm25_b as f32, stop_words)
                                .map_err(core_error)?,
                        )
                        .map_err(core_error)?,
                )),
                closed: AtomicBool::new(false),
            }),
        })
    }

    /// Creates an uninitialized handle used only by the TypeScript `load` path.
    #[napi(factory)]
    pub fn empty() -> Self {
        Self {
            shared: Arc::new(RetrievalShared {
                state: Mutex::new(RetrievalState::Empty),
                closed: AtomicBool::new(false),
            }),
        }
    }

    #[napi]
    pub fn add_documents(
        &self,
        documents: Vec<NativeDocumentInput>,
    ) -> Result<AsyncTask<AddDocumentsTask>> {
        self.shared.require_open()?;
        let documents = documents
            .into_iter()
            .map(NativeDocumentInput::into_owned)
            .collect::<Result<Vec<_>>>()?;
        Ok(AsyncTask::new(AddDocumentsTask {
            shared: Arc::clone(&self.shared),
            documents,
        }))
    }

    /// Private fixture/conformance surface. Public TypeScript ingestion never
    /// exposes chunk keys or keyed embedding dictionaries.
    #[napi(js_name = "_addFixtureRecords")]
    pub fn add_fixture_records(
        &self,
        records: Vec<NativeRecordInput>,
    ) -> Result<AsyncTask<AddFixtureRecordsTask>> {
        self.shared.require_open()?;
        let records = records
            .into_iter()
            .map(NativeRecordInput::into_owned)
            .collect::<Result<Vec<_>>>()?;
        Ok(AsyncTask::new(AddFixtureRecordsTask {
            shared: Arc::clone(&self.shared),
            records,
        }))
    }

    #[napi]
    pub fn build(&self) -> Result<AsyncTask<BuildRetrievalTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(BuildRetrievalTask {
            shared: Arc::clone(&self.shared),
        }))
    }

    #[napi]
    pub fn load(&self, path: String) -> Result<AsyncTask<LoadRetrievalTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(LoadRetrievalTask {
            shared: Arc::clone(&self.shared),
            path,
        }))
    }

    #[napi]
    pub fn semantic_search(
        &self,
        embedding: Float32Array,
        top_k: u32,
        filter: Option<NativeFilter>,
    ) -> Result<AsyncTask<SemanticSearchTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(SemanticSearchTask {
            shared: Arc::clone(&self.shared),
            embedding: embedding.to_vec(),
            top_k: top_k as usize,
            filter: filter.map(NativeFilter::into_core).transpose()?,
        }))
    }

    #[napi]
    pub fn keyword_search(
        &self,
        text: String,
        top_k: u32,
        filter: Option<NativeFilter>,
    ) -> Result<AsyncTask<KeywordSearchTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(KeywordSearchTask {
            shared: Arc::clone(&self.shared),
            text,
            top_k: top_k as usize,
            filter: filter.map(NativeFilter::into_core).transpose()?,
        }))
    }

    #[napi]
    #[allow(clippy::too_many_arguments)]
    pub fn hybrid_search(
        &self,
        text: String,
        embedding: Option<Float32Array>,
        top_k: u32,
        filter: Option<NativeFilter>,
        alpha: f64,
        vector_candidates: Option<u32>,
        keyword_candidates: Option<u32>,
    ) -> Result<AsyncTask<HybridSearchTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(HybridSearchTask {
            shared: Arc::clone(&self.shared),
            text,
            embedding: embedding.map(|value| value.to_vec()).unwrap_or_default(),
            top_k: top_k as usize,
            filter: filter.map(NativeFilter::into_core).transpose()?,
            alpha: alpha as f32,
            vector_candidates: vector_candidates.map(|value| value as usize),
            keyword_candidates: keyword_candidates.map(|value| value as usize),
        }))
    }

    #[napi]
    pub fn save(&self, path: String) -> Result<AsyncTask<SaveRetrievalTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(SaveRetrievalTask {
            shared: Arc::clone(&self.shared),
            path,
        }))
    }

    #[napi]
    pub fn close(&self) -> AsyncTask<CloseRetrievalTask> {
        self.shared.closed.store(true, Ordering::Release);
        AsyncTask::new(CloseRetrievalTask {
            shared: Arc::clone(&self.shared),
        })
    }

    #[napi(getter)]
    pub fn closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }
}

#[napi]
pub fn validate_retrieval(path: String) -> AsyncTask<ValidateRetrievalTask> {
    AsyncTask::new(ValidateRetrievalTask { path })
}

pub struct ValidateRetrievalTask {
    path: String,
}

impl Task for ValidateRetrievalTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        RetrievalDatabase::validate_dir(&self.path).map_err(core_error)
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct CloseRetrievalTask {
    shared: Arc<RetrievalShared>,
}

impl Task for CloseRetrievalTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        let mut state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval database lock was poisoned by a previous native failure")
        })?;
        *state = RetrievalState::Empty;
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct AddDocumentsTask {
    shared: Arc<RetrievalShared>,
    documents: Vec<OwnedDocumentInput>,
}

impl Task for AddDocumentsTask {
    type Output = Vec<Vec<u64>>;
    type JsValue = Vec<Vec<f64>>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let mut state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval builder lock was poisoned by a previous native failure")
        })?;
        let RetrievalState::Building(builder) = &mut *state else {
            return Err(state_error(
                "documents can only be added before build(); create a new builder to replace the corpus",
            ));
        };
        self.documents
            .drain(..)
            .map(|input| {
                builder
                    .upsert_document(input.document, input.embedding)
                    .map_err(core_error)
            })
            .collect()
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output
            .into_iter()
            .map(|ids| ids.into_iter().map(|id| id as f64).collect())
            .collect())
    }
}

pub struct AddFixtureRecordsTask {
    shared: Arc<RetrievalShared>,
    records: Vec<OwnedRecordInput>,
}

impl Task for AddFixtureRecordsTask {
    type Output = Vec<Vec<u64>>;
    type JsValue = Vec<Vec<f64>>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let mut state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval builder lock was poisoned by a previous native failure")
        })?;
        let RetrievalState::Building(builder) = &mut *state else {
            return Err(state_error(
                "fixture records can only be added before build()",
            ));
        };
        self.records
            .drain(..)
            .map(|input| {
                let (record, metadata, chunks) = crate::common::retrieval_chunks(input)?;
                builder
                    .upsert_record(record, metadata, chunks)
                    .map_err(core_error)
            })
            .collect()
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output
            .into_iter()
            .map(|ids| ids.into_iter().map(|id| id as f64).collect())
            .collect())
    }
}

pub struct BuildRetrievalTask {
    shared: Arc<RetrievalShared>,
}

impl Task for BuildRetrievalTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        self.shared.require_open()?;
        let mut state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval builder lock was poisoned by a previous native failure")
        })?;
        let current = std::mem::replace(&mut *state, RetrievalState::Empty);
        let RetrievalState::Building(builder) = current else {
            *state = current;
            return Err(state_error(
                "build() may be called exactly once on a retrieval builder",
            ));
        };
        match builder.build() {
            Ok(database) => {
                *state = RetrievalState::Ready(database);
                Ok(())
            }
            Err(error) => Err(core_error(error)),
        }
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct LoadRetrievalTask {
    shared: Arc<RetrievalShared>,
    path: String,
}

impl Task for LoadRetrievalTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        self.shared.require_open()?;
        let database = RetrievalDatabase::load_from_dir(&self.path).map_err(core_error)?;
        let mut state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval database lock was poisoned by a previous native failure")
        })?;
        *state = RetrievalState::Ready(database);
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct SemanticSearchTask {
    shared: Arc<RetrievalShared>,
    embedding: Vec<f32>,
    top_k: usize,
    filter: Option<retrievalkit_core::Filter>,
}

impl Task for SemanticSearchTask {
    type Output = Vec<NativeSearchHit>;
    type JsValue = Vec<NativeSearchHit>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval database lock was poisoned by a previous native failure")
        })?;
        let RetrievalState::Ready(database) = &*state else {
            return Err(state_error(
                "search requires a built or loaded retrieval database",
            ));
        };
        let mut query = SearchQuery::new(std::mem::take(&mut self.embedding), self.top_k);
        if let Some(filter) = self.filter.take() {
            query = query.with_filter(filter);
        }
        let hits = database.semantic_search(&query).map_err(core_error)?;
        search_hits(database, hits)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct KeywordSearchTask {
    shared: Arc<RetrievalShared>,
    text: String,
    top_k: usize,
    filter: Option<retrievalkit_core::Filter>,
}

impl Task for KeywordSearchTask {
    type Output = Vec<NativeKeywordHit>;
    type JsValue = Vec<NativeKeywordHit>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval database lock was poisoned by a previous native failure")
        })?;
        let RetrievalState::Ready(database) = &*state else {
            return Err(state_error(
                "search requires a built or loaded retrieval database",
            ));
        };
        let mut query = KeywordQuery::new(std::mem::take(&mut self.text), self.top_k);
        if let Some(filter) = self.filter.take() {
            query = query.with_filter(filter);
        }
        let hits = database.keyword_search(&query).map_err(core_error)?;
        keyword_hits(database, hits)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct HybridSearchTask {
    shared: Arc<RetrievalShared>,
    text: String,
    embedding: Vec<f32>,
    top_k: usize,
    filter: Option<retrievalkit_core::Filter>,
    alpha: f32,
    vector_candidates: Option<usize>,
    keyword_candidates: Option<usize>,
}

impl Task for HybridSearchTask {
    type Output = Vec<NativeHybridHit>;
    type JsValue = Vec<NativeHybridHit>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval database lock was poisoned by a previous native failure")
        })?;
        let RetrievalState::Ready(database) = &*state else {
            return Err(state_error(
                "search requires a built or loaded retrieval database",
            ));
        };
        let mut query = HybridQuery::new(
            std::mem::take(&mut self.text),
            std::mem::take(&mut self.embedding),
            self.top_k,
        )
        .try_with_alpha(self.alpha)
        .map_err(core_error)?;
        let vector_top_k = self.vector_candidates.unwrap_or(query.vector_top_k);
        let keyword_top_k = self.keyword_candidates.unwrap_or(query.keyword_top_k);
        query = query.with_candidate_limits(vector_top_k, keyword_top_k);
        if let Some(filter) = self.filter.take() {
            query = query.with_filter(filter);
        }
        let hits = database.hybrid_search(&query).map_err(core_error)?;
        hybrid_hits(database, hits, self.alpha)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct SaveRetrievalTask {
    shared: Arc<RetrievalShared>,
    path: String,
}

impl Task for SaveRetrievalTask {
    type Output = NativeFileSizeReport;
    type JsValue = NativeFileSizeReport;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = self.shared.state.lock().map_err(|_| {
            state_error("retrieval database lock was poisoned by a previous native failure")
        })?;
        let RetrievalState::Ready(database) = &*state else {
            return Err(state_error(
                "save requires a built or loaded retrieval database",
            ));
        };
        database
            .save_to_dir(&self.path)
            .map(NativeFileSizeReport::from)
            .map_err(core_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub(crate) fn search_hits(
    database: &RetrievalDatabase,
    hits: Vec<SearchHit>,
) -> Result<Vec<NativeSearchHit>> {
    hits.into_iter()
        .map(|hit| {
            let chunk = database.chunk(hit.chunk_id).ok_or_else(|| {
                state_error("Rust returned a search hit whose canonical chunk is unavailable")
            })?;
            Ok(NativeSearchHit {
                document_id: hit.document_id,
                text: chunk.text.clone(),
                metadata: metadata_to_native(&chunk.metadata),
                score: f64::from(hit.score),
                vector_score: f64::from(hit.trace.vector_score),
            })
        })
        .collect()
}

pub(crate) fn keyword_hits(
    database: &RetrievalDatabase,
    hits: Vec<KeywordHit>,
) -> Result<Vec<NativeKeywordHit>> {
    hits.into_iter()
        .map(|hit| {
            let chunk = database.chunk(hit.chunk_id).ok_or_else(|| {
                state_error("Rust returned a keyword hit whose canonical chunk is unavailable")
            })?;
            Ok(NativeKeywordHit {
                document_id: hit.document_id,
                text: chunk.text.clone(),
                metadata: metadata_to_native(&chunk.metadata),
                score: f64::from(hit.score),
                matched_terms: hit.matched_terms,
            })
        })
        .collect()
}

pub(crate) fn hybrid_hits(
    database: &RetrievalDatabase,
    hits: Vec<HybridHit>,
    requested_alpha: f32,
) -> Result<Vec<NativeHybridHit>> {
    hits.into_iter()
        .map(|hit| {
            let chunk = database.chunk(hit.chunk_id).ok_or_else(|| {
                state_error("Rust returned a hybrid hit whose canonical chunk is unavailable")
            })?;
            let alpha = match hit.trace.fusion {
                HybridFusionTrace::WeightedNormalizedScore { vector_weight, .. } => vector_weight,
                HybridFusionTrace::ReciprocalRank { .. } => requested_alpha,
            };
            Ok(NativeHybridHit {
                document_id: hit.document_id,
                text: chunk.text.clone(),
                metadata: metadata_to_native(&chunk.metadata),
                score: f64::from(hit.score),
                vector_score: hit.vector_score.map(f64::from),
                keyword_score: hit.keyword_score.map(f64::from),
                trace: NativeHybridTrace {
                    alpha: f64::from(alpha),
                    vector_rank: hit.trace.vector_rank.map(|value| value as u32),
                    keyword_rank: hit.trace.keyword_rank.map(|value| value as u32),
                    normalized_vector_score: hit.trace.normalized_vector_score.map(f64::from),
                    normalized_keyword_score: hit.trace.normalized_keyword_score.map(f64::from),
                    matched_terms: hit.trace.matched_terms,
                },
            })
        })
        .collect()
}
