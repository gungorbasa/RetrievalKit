use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bm25::{Bm25Config, Bm25Index, PersistedBm25Index};
use crate::error::{Result, VectorKitError};
use crate::filter::Filter;
use crate::metadata::{estimated_metadata_payload_bytes, Metadata};
use crate::metadata_index::MetadataFilterIndex;
use crate::scoring::{self, EncodedVectorStore};
use crate::types::{
    Chunk, ChunkId, ChunkInput, Document, IndexConfig, IndexFileSizeReport, IndexSizeEstimate,
    KeywordHit, KeywordQuery, SearchHit, SearchQuery, SearchTrace, StoredChunk, VectorEncoding,
    VectorMetric,
};

const FORMAT_VERSION: u32 = 1;
const CREATED_WITH: &str = "vectorkit";
const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_TMP_FILE: &str = "manifest.json.tmp";
const VECTORS_FILE: &str = "vectors.vec";
const CHUNKS_FILE: &str = "chunks.bin";
const BM25_FILE: &str = "bm25.bin";
const TOMBSTONES_FILE: &str = "tombstones.bin";

#[derive(Debug, Clone)]
pub struct ExactVectorIndex {
    dimension: usize,
    metric: VectorMetric,
    vector_encoding: VectorEncoding,
    chunks: Vec<StoredChunk>,
    encoded_vectors: EncodedVectorStore,
    chunk_offsets: Vec<Option<usize>>,
    metadata_filter_index: MetadataFilterIndex,
    next_chunk_id: ChunkId,
    document_versions: BTreeMap<String, u64>,
    bm25: Bm25Index,
}

impl ExactVectorIndex {
    /// Creates an empty exact vector index with a fixed embedding dimension.
    pub fn new(dimension: usize, metric: VectorMetric) -> Self {
        Self::with_bm25_config(dimension, metric, Bm25Config::default())
    }

    /// Creates an empty exact vector index with configured vector storage.
    pub fn try_with_config(config: IndexConfig) -> Result<Self> {
        Self::try_with_config_and_bm25(config, Bm25Config::default())
    }

    /// Creates an empty exact vector index with configured vector and BM25 settings.
    pub fn try_with_config_and_bm25(config: IndexConfig, bm25_config: Bm25Config) -> Result<Self> {
        Self::from_parts(
            config.dimension,
            config.metric,
            config.vector_encoding,
            bm25_config,
        )
    }

    /// Creates an empty exact vector index with custom BM25 settings.
    pub fn with_bm25_config(
        dimension: usize,
        metric: VectorMetric,
        bm25_config: Bm25Config,
    ) -> Self {
        Self::from_parts(dimension, metric, VectorEncoding::F32, bm25_config)
            .expect("F32 vector encoding is supported")
    }

    fn from_parts(
        dimension: usize,
        metric: VectorMetric,
        vector_encoding: VectorEncoding,
        bm25_config: Bm25Config,
    ) -> Result<Self> {
        Ok(Self {
            dimension,
            metric,
            vector_encoding,
            chunks: Vec::new(),
            encoded_vectors: EncodedVectorStore::new(vector_encoding)?,
            chunk_offsets: Vec::new(),
            metadata_filter_index: MetadataFilterIndex::default(),
            next_chunk_id: 0,
            document_versions: BTreeMap::new(),
            bm25: Bm25Index::new(bm25_config),
        })
    }

    /// Returns the required embedding dimension for indexed chunks and queries.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Returns the vector metric used for scoring.
    pub fn metric(&self) -> VectorMetric {
        self.metric
    }

    /// Returns the stored vector representation used by this index.
    pub fn vector_encoding(&self) -> VectorEncoding {
        self.vector_encoding
    }

    /// Returns the total number of stored chunks, including tombstoned chunks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Returns true when no chunks have been stored.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Returns the number of chunks currently eligible for search results.
    pub fn active_chunk_count(&self) -> usize {
        self.chunks.iter().filter(|chunk| !chunk.deleted).count()
    }

    /// Returns an approximate payload byte breakdown for the currently loaded index.
    pub fn size_estimate(&self) -> IndexSizeEstimate {
        IndexSizeEstimate {
            vector_bytes: self.encoded_vectors.estimated_payload_bytes(),
            chunk_record_bytes: self.chunks.len() * std::mem::size_of::<ChunkId>(),
            document_id_bytes: self
                .chunks
                .iter()
                .map(|chunk| chunk.document_id.len())
                .sum(),
            text_bytes: self.chunks.iter().map(|chunk| chunk.text.len()).sum(),
            metadata_bytes: self
                .chunks
                .iter()
                .map(|chunk| estimated_metadata_payload_bytes(&chunk.metadata))
                .sum(),
            tombstone_bytes: self.chunks.len() * std::mem::size_of::<bool>(),
            version_bytes: self.chunks.len() * std::mem::size_of::<u64>(),
            chunk_offset_bytes: self.chunk_offsets.len() * std::mem::size_of::<Option<usize>>(),
            bm25_bytes: self.bm25.estimated_payload_bytes(),
            metadata_filter_bytes: self.metadata_filter_index.estimated_payload_bytes(),
        }
    }

    /// Saves the loaded index to a local directory and returns actual file sizes.
    pub fn save_to_dir(&self, directory: impl AsRef<Path>) -> Result<IndexFileSizeReport> {
        let directory = directory.as_ref();
        fs::create_dir_all(directory)
            .map_err(|_| persistence_error("create directory", directory))?;

        let vectors_path = directory.join(VECTORS_FILE);
        let chunks_path = directory.join(CHUNKS_FILE);
        let bm25_path = directory.join(BM25_FILE);
        let tombstones_path = directory.join(TOMBSTONES_FILE);
        let manifest_path = directory.join(MANIFEST_FILE);
        let manifest_tmp_path = directory.join(MANIFEST_TMP_FILE);

        write_file(&vectors_path, &self.encoded_vectors.to_payload_bytes())?;
        write_json_file(&chunks_path, &self.chunks)?;
        write_json_file(&bm25_path, &self.bm25.to_persisted())?;
        write_file(
            &tombstones_path,
            &self
                .chunks
                .iter()
                .map(|chunk| u8::from(chunk.deleted))
                .collect::<Vec<_>>(),
        )?;

        let manifest = PersistedManifest {
            format_version: FORMAT_VERSION,
            created_with: CREATED_WITH.to_owned(),
            dimension: self.dimension,
            metric: self.metric,
            vector_count: self.chunks.len(),
            active_chunk_count: self.active_chunk_count(),
            has_bm25: true,
            vector_encoding: self.vector_encoding,
            vector_bytes: self.encoded_vectors.estimated_payload_bytes(),
            chunk_bytes: file_size(&chunks_path)?,
            bm25_bytes: file_size(&bm25_path)?,
            tombstone_bytes: file_size(&tombstones_path)?,
            normalization: match self.metric {
                VectorMetric::Cosine => "unit_l2",
                VectorMetric::DotProduct => "none",
            }
            .to_owned(),
        };

        write_json_file(&manifest_tmp_path, &manifest)?;
        fs::rename(&manifest_tmp_path, &manifest_path)
            .map_err(|_| persistence_error("publish manifest", &manifest_path))?;

        Self::persisted_file_sizes(directory)
    }

    /// Loads a previously saved local index directory.
    pub fn load_from_dir(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        let manifest_path = directory.join(MANIFEST_FILE);
        let manifest: PersistedManifest = read_json_file(&manifest_path)?;
        manifest.validate()?;

        let vectors_path = directory.join(VECTORS_FILE);
        let chunks_path = directory.join(CHUNKS_FILE);
        let bm25_path = directory.join(BM25_FILE);
        let tombstones_path = directory.join(TOMBSTONES_FILE);

        validate_file_size(&chunks_path, manifest.chunk_bytes)?;
        validate_file_size(&bm25_path, manifest.bm25_bytes)?;
        validate_file_size(&tombstones_path, manifest.tombstone_bytes)?;
        let vector_bytes = read_file(&vectors_path)?;
        if vector_bytes.len() != manifest.vector_bytes {
            return Err(VectorKitError::InvalidFormat {
                message: format!(
                    "manifest vector bytes {} do not match vectors file bytes {}",
                    manifest.vector_bytes,
                    vector_bytes.len()
                ),
            });
        }

        let encoded_vectors = EncodedVectorStore::from_payload_bytes(
            manifest.vector_encoding,
            manifest.vector_count,
            manifest.dimension,
            &vector_bytes,
        )?;
        let chunks: Vec<StoredChunk> = read_json_file(&chunks_path)?;
        if chunks.len() != manifest.vector_count {
            return Err(VectorKitError::InvalidFormat {
                message: format!(
                    "manifest vector count {} does not match chunk count {}",
                    manifest.vector_count,
                    chunks.len()
                ),
            });
        }

        let tombstones = read_file(&tombstones_path)?;
        if tombstones.len() != chunks.len() {
            return Err(VectorKitError::InvalidFormat {
                message: format!(
                    "tombstone count {} does not match chunk count {}",
                    tombstones.len(),
                    chunks.len()
                ),
            });
        }
        for (offset, (chunk, tombstone)) in chunks.iter().zip(&tombstones).enumerate() {
            if chunk.deleted != (*tombstone != 0) {
                return Err(VectorKitError::InvalidFormat {
                    message: format!("chunk {offset} tombstone does not match chunk record"),
                });
            }
        }
        let persisted_bm25: PersistedBm25Index = read_json_file(&bm25_path)?;
        validate_bm25_state_matches_chunks(&persisted_bm25, &chunks)?;

        let mut index = Self::from_parts(
            manifest.dimension,
            manifest.metric,
            manifest.vector_encoding,
            Bm25Config::default(),
        )?;
        index.encoded_vectors = encoded_vectors;
        index.chunks = chunks;
        index.bm25 = Bm25Index::from_persisted(Bm25Config::default(), persisted_bm25)?;
        index.rebuild_derived_state_from_loaded_bm25();

        if index.active_chunk_count() != manifest.active_chunk_count {
            return Err(VectorKitError::InvalidFormat {
                message: format!(
                    "manifest active chunk count {} does not match loaded active chunk count {}",
                    manifest.active_chunk_count,
                    index.active_chunk_count()
                ),
            });
        }

        Ok(index)
    }

    /// Returns actual file sizes for a saved index directory.
    pub fn persisted_file_sizes(directory: impl AsRef<Path>) -> Result<IndexFileSizeReport> {
        let directory = directory.as_ref();
        Ok(IndexFileSizeReport {
            manifest_bytes: file_size(&directory.join(MANIFEST_FILE))?,
            vectors_bytes: file_size(&directory.join(VECTORS_FILE))?,
            chunks_bytes: file_size(&directory.join(CHUNKS_FILE))?,
            bm25_bytes: file_size(&directory.join(BM25_FILE))?,
            tombstones_bytes: file_size(&directory.join(TOMBSTONES_FILE))?,
        })
    }

    /// Adds a prebuilt chunk directly.
    ///
    /// Most callers should use `upsert_document` so VectorKit can assign
    /// internal chunk IDs and enforce document version tombstones. This method
    /// remains useful for tests and future persistence-loading paths.
    pub fn add_chunk(&mut self, chunk: Chunk) -> Result<()> {
        self.validate_dimension(chunk.embedding.len())?;
        let offset = self.chunks.len();
        self.next_chunk_id = self.next_chunk_id.max(chunk.chunk_id.saturating_add(1));
        self.document_versions
            .entry(chunk.document_id.clone())
            .and_modify(|version| *version = (*version).max(chunk.version))
            .or_insert(chunk.version);
        self.bm25
            .add_chunk(chunk.chunk_id, &chunk.text, !chunk.deleted);
        self.register_chunk_offset(chunk.chunk_id, offset);
        self.push_embedding(&chunk.embedding);
        let stored_chunk = StoredChunk {
            chunk_id: chunk.chunk_id,
            document_id: chunk.document_id,
            text: chunk.text,
            metadata: chunk.metadata,
            deleted: chunk.deleted,
            version: chunk.version,
        };
        if !stored_chunk.deleted {
            self.metadata_filter_index
                .insert(offset, &stored_chunk.metadata);
        }
        self.chunks.push(stored_chunk);
        Ok(())
    }

    /// Adds or replaces all chunks for a caller-owned document ID.
    ///
    /// Existing chunks for the document are tombstoned before new chunks are
    /// appended. The returned `ChunkId` values are internal IDs assigned by the
    /// index and are stable for those stored chunks.
    pub fn upsert_document(
        &mut self,
        document: Document,
        chunk_inputs: Vec<ChunkInput>,
    ) -> Result<Vec<ChunkId>> {
        for chunk in &chunk_inputs {
            self.validate_dimension(chunk.embedding.len())?;
        }
        self.encoded_vectors
            .reserve_rows(chunk_inputs.len(), self.dimension);

        let version = self
            .document_versions
            .get(&document.id)
            .copied()
            .unwrap_or(0)
            + 1;

        for (offset, chunk) in self.chunks.iter_mut().enumerate() {
            if chunk.document_id == document.id {
                if !chunk.deleted {
                    self.metadata_filter_index.remove(offset, &chunk.metadata);
                }
                chunk.deleted = true;
                self.bm25.deactivate_chunk(chunk.chunk_id);
            }
        }

        let mut chunk_ids = Vec::with_capacity(chunk_inputs.len());
        for chunk_input in chunk_inputs {
            let offset = self.chunks.len();
            let chunk_id = self.allocate_chunk_id();
            chunk_ids.push(chunk_id);
            self.bm25.add_chunk(chunk_id, &chunk_input.text, true);
            self.register_chunk_offset(chunk_id, offset);
            self.push_embedding(&chunk_input.embedding);
            let stored_chunk = StoredChunk {
                chunk_id,
                document_id: document.id.clone(),
                text: chunk_input.text,
                metadata: merge_metadata(&document.metadata, chunk_input.metadata),
                deleted: false,
                version,
            };
            self.metadata_filter_index
                .insert(offset, &stored_chunk.metadata);
            self.chunks.push(stored_chunk);
        }

        self.document_versions.insert(document.id, version);

        Ok(chunk_ids)
    }

    /// Tombstones all active chunks for a caller-owned document ID.
    ///
    /// Returns the number of chunks newly marked deleted. Repeated deletes are
    /// idempotent and return zero once no active chunks remain.
    pub fn delete_document(&mut self, document_id: &str) -> usize {
        let mut deleted_count = 0;
        for (offset, chunk) in self.chunks.iter_mut().enumerate() {
            if chunk.document_id == document_id && !chunk.deleted {
                self.metadata_filter_index.remove(offset, &chunk.metadata);
                chunk.deleted = true;
                self.bm25.deactivate_chunk(chunk.chunk_id);
                deleted_count += 1;
            }
        }
        deleted_count
    }

    /// Returns a stored chunk by its internal ID.
    pub fn chunk(&self, chunk_id: ChunkId) -> Option<&StoredChunk> {
        let offset = self
            .chunk_offsets
            .get(usize::try_from(chunk_id).ok()?)?
            .as_ref()?;
        self.chunks.get(*offset)
    }

    /// Performs exact vector search over active chunks.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.validate_dimension(query.embedding.len())?;

        if query.top_k == 0 {
            return Ok(Vec::new());
        }
        let encoded_query = self.encode_query_embedding(&query.embedding)?;

        let mut hits = Vec::with_capacity(query.top_k);
        let candidate_offsets = query
            .filter
            .as_ref()
            .map(|filter| self.metadata_filter_index.candidate_offsets(filter))
            .transpose()?
            .flatten();

        match candidate_offsets {
            Some(offsets) => {
                for offset in offsets {
                    self.score_search_candidate(offset, query, &encoded_query, &mut hits)?;
                }
            }
            None => {
                for offset in 0..self.chunks.len() {
                    self.score_search_candidate(offset, query, &encoded_query, &mut hits)?;
                }
            }
        }

        sort_hits(&mut hits);

        Ok(hits)
    }

    /// Performs BM25 keyword search over active chunks.
    pub fn keyword_search(&self, query: &KeywordQuery) -> Result<Vec<KeywordHit>> {
        if query.top_k == 0 {
            return Ok(Vec::new());
        }

        let mut hits = Vec::new();
        for keyword_hit in self.bm25.search_all(&query.text) {
            let Some(chunk) = self.chunk(keyword_hit.chunk_id) else {
                continue;
            };

            if chunk.deleted {
                continue;
            }

            if !matches_filter(query.filter.as_ref(), chunk)? {
                continue;
            }

            hits.push(KeywordHit {
                chunk_id: chunk.chunk_id,
                document_id: chunk.document_id.clone(),
                score: keyword_hit.score,
                matched_terms: keyword_hit.matched_terms,
            });

            if hits.len() == query.top_k {
                break;
            }
        }

        Ok(hits)
    }

    fn validate_dimension(&self, actual: usize) -> Result<()> {
        if actual == self.dimension {
            Ok(())
        } else {
            Err(VectorKitError::InvalidDimension {
                expected: self.dimension,
                actual,
            })
        }
    }

    fn allocate_chunk_id(&mut self) -> ChunkId {
        let chunk_id = self.next_chunk_id;
        self.next_chunk_id = self.next_chunk_id.saturating_add(1);
        chunk_id
    }

    fn register_chunk_offset(&mut self, chunk_id: ChunkId, offset: usize) {
        let Some(chunk_id) = usize::try_from(chunk_id).ok() else {
            return;
        };

        if self.chunk_offsets.len() <= chunk_id {
            self.chunk_offsets.resize(chunk_id + 1, None);
        }
        self.chunk_offsets[chunk_id] = Some(offset);
    }

    fn push_embedding(&mut self, embedding: &[f32]) {
        match self.metric {
            VectorMetric::DotProduct => self.encoded_vectors.push(embedding),
            VectorMetric::Cosine => {
                let mut normalized = embedding.to_vec();
                scoring::normalize(&mut normalized);
                self.encoded_vectors.push(&normalized);
            }
        }
    }

    fn encode_query_embedding(&self, embedding: &[f32]) -> Result<scoring::EncodedQuery> {
        match self.metric {
            VectorMetric::DotProduct => scoring::encode_query(self.vector_encoding, embedding),
            VectorMetric::Cosine => {
                let mut normalized = embedding.to_vec();
                scoring::normalize(&mut normalized);
                scoring::encode_query_owned(self.vector_encoding, normalized)
            }
        }
    }

    fn rebuild_derived_state_from_loaded_bm25(&mut self) {
        self.chunk_offsets.clear();
        self.metadata_filter_index = MetadataFilterIndex::default();
        self.document_versions.clear();
        self.next_chunk_id = 0;

        for offset in 0..self.chunks.len() {
            let chunk_id = self.chunks[offset].chunk_id;
            self.next_chunk_id = self.next_chunk_id.max(chunk_id.saturating_add(1));
            self.register_chunk_offset(chunk_id, offset);

            let chunk = &self.chunks[offset];
            self.document_versions
                .entry(chunk.document_id.clone())
                .and_modify(|version| *version = (*version).max(chunk.version))
                .or_insert(chunk.version);
            if !chunk.deleted {
                self.metadata_filter_index.insert(offset, &chunk.metadata);
            }
        }
    }

    fn score_search_candidate(
        &self,
        offset: usize,
        query: &SearchQuery,
        encoded_query: &scoring::EncodedQuery,
        hits: &mut Vec<SearchHit>,
    ) -> Result<()> {
        let Some(chunk) = self.chunks.get(offset) else {
            return Ok(());
        };

        if chunk.deleted {
            return Ok(());
        }

        if !matches_filter(query.filter.as_ref(), chunk)? {
            return Ok(());
        }

        let Some(score) =
            self.encoded_vectors
                .score_at(self.metric, encoded_query, offset, self.dimension)
        else {
            return Ok(());
        };

        push_bounded_hit(
            hits,
            query.top_k,
            SearchHit {
                chunk_id: chunk.chunk_id,
                document_id: chunk.document_id.clone(),
                score,
                trace: SearchTrace {
                    vector_score: score,
                    keyword_score: None,
                    filter_matched: true,
                },
            },
        );

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedManifest {
    format_version: u32,
    created_with: String,
    dimension: usize,
    metric: VectorMetric,
    vector_count: usize,
    active_chunk_count: usize,
    has_bm25: bool,
    vector_encoding: VectorEncoding,
    vector_bytes: usize,
    chunk_bytes: u64,
    bm25_bytes: u64,
    tombstone_bytes: u64,
    normalization: String,
}

impl PersistedManifest {
    fn validate(&self) -> Result<()> {
        if self.format_version != FORMAT_VERSION {
            return Err(VectorKitError::InvalidFormat {
                message: format!("unsupported format version {}", self.format_version),
            });
        }

        if self.created_with != CREATED_WITH {
            return Err(VectorKitError::InvalidFormat {
                message: format!("unsupported index creator '{}'", self.created_with),
            });
        }

        Ok(())
    }
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| VectorKitError::InvalidFormat {
            message: format!("could not serialize '{}': {error}", path.display()),
        })?;
    write_file(path, &bytes)
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = read_file(path)?;
    serde_json::from_slice(&bytes).map_err(|error| VectorKitError::InvalidFormat {
        message: format!("could not parse '{}': {error}", path.display()),
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).map_err(|_| persistence_error("write", path))
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|_| persistence_error("read", path))
}

fn file_size(path: &Path) -> Result<u64> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|_| persistence_error("stat", path))
}

fn validate_file_size(path: &Path, expected: u64) -> Result<()> {
    let actual = file_size(path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(VectorKitError::InvalidFormat {
            message: format!(
                "manifest file size {expected} does not match '{}' size {actual}",
                path.display()
            ),
        })
    }
}

fn validate_bm25_state_matches_chunks(
    bm25: &PersistedBm25Index,
    chunks: &[StoredChunk],
) -> Result<()> {
    let chunk_by_id = chunks
        .iter()
        .map(|chunk| (chunk.chunk_id, chunk))
        .collect::<BTreeMap<_, _>>();

    for chunk_id in bm25.active_chunk_ids() {
        let Some(chunk) = chunk_by_id.get(chunk_id) else {
            return Err(VectorKitError::InvalidFormat {
                message: format!("bm25 active chunk {chunk_id} is missing from chunk records"),
            });
        };

        if chunk.deleted {
            return Err(VectorKitError::InvalidFormat {
                message: format!("bm25 active chunk {chunk_id} is tombstoned"),
            });
        }
    }

    for chunk_id in bm25.chunk_length_ids() {
        if !chunk_by_id.contains_key(chunk_id) {
            return Err(VectorKitError::InvalidFormat {
                message: format!("bm25 length chunk {chunk_id} is missing from chunk records"),
            });
        }
    }

    Ok(())
}

fn persistence_error(operation: &str, path: &Path) -> VectorKitError {
    VectorKitError::Persistence {
        operation: operation.to_owned(),
        path: display_path(path),
    }
}

fn display_path(path: &Path) -> String {
    PathBuf::from(path).display().to_string()
}

fn matches_filter(filter: Option<&Filter>, chunk: &StoredChunk) -> Result<bool> {
    match filter {
        Some(filter) => filter.matches(&chunk.metadata),
        None => Ok(true),
    }
}

fn merge_metadata(document_metadata: &Metadata, chunk_metadata: Metadata) -> Metadata {
    let mut metadata = document_metadata.clone();
    metadata.extend(chunk_metadata);
    metadata
}

fn push_bounded_hit(hits: &mut Vec<SearchHit>, top_k: usize, candidate: SearchHit) {
    if hits.len() < top_k {
        hits.push(candidate);
        return;
    }

    let Some(worst_index) = worst_hit_index(hits) else {
        return;
    };

    if hit_ranks_before(&candidate, &hits[worst_index]) {
        hits[worst_index] = candidate;
    }
}

fn worst_hit_index(hits: &[SearchHit]) -> Option<usize> {
    let mut worst_index = 0;
    for index in 1..hits.len() {
        if hit_ranks_before(&hits[worst_index], &hits[index]) {
            worst_index = index;
        }
    }
    Some(worst_index)
}

fn sort_hits(hits: &mut [SearchHit]) {
    hits.sort_by(compare_hits);
}

fn compare_hits(left: &SearchHit, right: &SearchHit) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn hit_ranks_before(left: &SearchHit, right: &SearchHit) -> bool {
    compare_hits(left, right).is_lt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{Metadata, MetadataValue};

    fn chunk(chunk_id: ChunkId, document_id: &str, embedding: Vec<f32>) -> Chunk {
        Chunk {
            chunk_id,
            document_id: document_id.to_owned(),
            text: format!("chunk {chunk_id}"),
            embedding,
            metadata: Metadata::new(),
            deleted: false,
            version: 1,
        }
    }

    fn document(document_id: &str) -> Document {
        Document {
            id: document_id.to_owned(),
            text: format!("document {document_id}"),
            metadata: Metadata::new(),
        }
    }

    fn chunk_input(text: &str, embedding: Vec<f32>) -> ChunkInput {
        ChunkInput {
            text: text.to_owned(),
            embedding,
            metadata: Metadata::new(),
        }
    }

    fn temp_index_dir(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("vectorkit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    fn assert_close(left: f32, right: f32) {
        assert!(
            (left - right).abs() <= 1e-5,
            "expected {left} to be close to {right}"
        );
    }

    #[test]
    fn rejects_chunks_with_wrong_dimension() {
        let mut index = ExactVectorIndex::new(3, VectorMetric::DotProduct);
        let error = index
            .add_chunk(chunk(1, "doc-1", vec![1.0, 0.0]))
            .unwrap_err();

        assert_eq!(
            error,
            VectorKitError::InvalidDimension {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn rejects_queries_with_wrong_dimension() {
        let index = ExactVectorIndex::new(3, VectorMetric::DotProduct);
        let query = SearchQuery::new(vec![1.0, 0.0], 10);

        let error = index.search(&query).unwrap_err();

        assert_eq!(
            error,
            VectorKitError::InvalidDimension {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn exact_search_excludes_deleted_chunks() {
        let mut index = ExactVectorIndex::new(3, VectorMetric::DotProduct);
        index
            .add_chunk(chunk(1, "doc-1", vec![1.0, 0.0, 0.0]))
            .unwrap();

        let mut deleted = chunk(2, "doc-2", vec![2.0, 0.0, 0.0]);
        deleted.deleted = true;
        index.add_chunk(deleted).unwrap();

        let hits = index
            .search(&SearchQuery::new(vec![1.0, 0.0, 0.0], 10))
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
    }

    #[test]
    fn active_chunk_count_excludes_deleted_chunks() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index.add_chunk(chunk(1, "doc-1", vec![1.0, 0.0])).unwrap();

        let mut deleted = chunk(2, "doc-2", vec![0.0, 1.0]);
        deleted.deleted = true;
        index.add_chunk(deleted).unwrap();

        assert_eq!(index.len(), 2);
        assert_eq!(index.active_chunk_count(), 1);
    }

    #[test]
    fn size_estimate_reports_current_payload_components() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut indexed_chunk = chunk(1, "doc-1", vec![1.0, 0.0]);
        indexed_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index.add_chunk(indexed_chunk).unwrap();

        let estimate = index.size_estimate();

        assert_eq!(estimate.vector_bytes, 2 * std::mem::size_of::<f32>());
        assert_eq!(estimate.chunk_record_bytes, std::mem::size_of::<ChunkId>());
        assert_eq!(estimate.document_id_bytes, "doc-1".len());
        assert_eq!(estimate.text_bytes, "chunk 1".len());
        assert_eq!(estimate.metadata_bytes, "source".len() + "notes".len());
        assert_eq!(estimate.tombstone_bytes, std::mem::size_of::<bool>());
        assert_eq!(estimate.version_bytes, std::mem::size_of::<u64>());
        assert_eq!(
            estimate.chunk_offset_bytes,
            2 * std::mem::size_of::<Option<usize>>()
        );
        assert!(estimate.bm25_bytes > 0);
        assert!(estimate.metadata_filter_bytes > 0);
        assert_eq!(
            estimate.total_bytes(),
            estimate.vector_bytes + estimate.auxiliary_bytes()
        );
    }

    #[test]
    fn saved_index_round_trips_vector_keyword_filter_and_tombstones() {
        let directory = temp_index_dir("round-trip");
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);

        let mut notes_document = document("doc-1");
        notes_document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index
            .upsert_document(
                notes_document,
                vec![chunk_input("Swift exact local search", vec![1.0, 0.0])],
            )
            .unwrap();

        let mut transcript_document = document("doc-2");
        transcript_document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index
            .upsert_document(
                transcript_document,
                vec![chunk_input("Rust vector core", vec![0.0, 1.0])],
            )
            .unwrap();

        index.delete_document("doc-2");

        let file_sizes = index.save_to_dir(&directory).unwrap();
        assert!(file_sizes.manifest_bytes > 0);
        assert!(file_sizes.vectors_bytes > 0);
        assert!(file_sizes.chunks_bytes > 0);
        assert!(file_sizes.bm25_bytes > 100);
        assert_eq!(file_sizes.tombstones_bytes, 2);
        assert_eq!(
            ExactVectorIndex::persisted_file_sizes(&directory).unwrap(),
            file_sizes
        );

        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
        assert_eq!(loaded.dimension(), 2);
        assert_eq!(loaded.metric(), VectorMetric::Cosine);
        assert_eq!(loaded.vector_encoding(), VectorEncoding::F32);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.active_chunk_count(), 1);
        assert!(loaded.chunk(1).unwrap().deleted);

        let vector_hits = loaded
            .search(&SearchQuery::new(vec![1.0, 0.0], 10))
            .unwrap();
        assert_eq!(
            vector_hits
                .iter()
                .map(|hit| hit.chunk_id)
                .collect::<Vec<_>>(),
            vec![0]
        );

        let keyword_hits = loaded
            .keyword_search(&KeywordQuery::new("swift local", 10))
            .unwrap();
        assert_eq!(keyword_hits.len(), 1);
        assert_eq!(keyword_hits[0].chunk_id, 0);

        let filtered_hits = loaded
            .search(
                &SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::Equals {
                    field: "source".to_owned(),
                    value: MetadataValue::String("notes".to_owned()),
                }),
            )
            .unwrap();
        assert_eq!(filtered_hits.len(), 1);
        assert_eq!(filtered_hits[0].chunk_id, 0);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn saved_i8_index_round_trips_encoded_vector_search() {
        let directory = temp_index_dir("i8-round-trip");
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::Cosine)
                .with_vector_encoding(VectorEncoding::I8ScalarQuantized),
        )
        .unwrap();
        index.add_chunk(chunk(0, "doc-1", vec![1.0, 0.0])).unwrap();
        index.add_chunk(chunk(1, "doc-2", vec![0.0, 1.0])).unwrap();

        index.save_to_dir(&directory).unwrap();
        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();

        assert_eq!(loaded.vector_encoding(), VectorEncoding::I8ScalarQuantized);
        let hits = loaded.search(&SearchQuery::new(vec![0.0, 1.0], 1)).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn exact_search_is_deterministic_for_tied_scores() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .add_chunk(chunk(20, "doc-20", vec![1.0, 0.0]))
            .unwrap();
        index
            .add_chunk(chunk(10, "doc-10", vec![1.0, 0.0]))
            .unwrap();

        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    #[test]
    fn exact_search_keeps_bounded_top_k_with_stable_ordering() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .add_chunk(chunk(40, "doc-40", vec![4.0, 0.0]))
            .unwrap();
        index
            .add_chunk(chunk(10, "doc-10", vec![1.0, 0.0]))
            .unwrap();
        index
            .add_chunk(chunk(20, "doc-20", vec![4.0, 0.0]))
            .unwrap();
        index
            .add_chunk(chunk(30, "doc-30", vec![3.0, 0.0]))
            .unwrap();

        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 2)).unwrap();

        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![20, 40]
        );
    }

    #[test]
    fn exact_search_applies_metadata_filters() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut notes_chunk = chunk(1, "doc-1", vec![1.0, 0.0]);
        notes_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index.add_chunk(notes_chunk).unwrap();

        let mut transcript_chunk = chunk(2, "doc-2", vec![2.0, 0.0]);
        transcript_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index.add_chunk(transcript_chunk).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("notes".to_owned()),
        });

        let hits = index.search(&query).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
    }

    #[test]
    fn exact_search_applies_metadata_filters_before_bounded_top_k() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut filtered_out = chunk(1, "doc-1", vec![10.0, 0.0]);
        filtered_out.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index.add_chunk(filtered_out).unwrap();

        let mut matching = chunk(2, "doc-2", vec![1.0, 0.0]);
        matching.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index.add_chunk(matching).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 1).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("notes".to_owned()),
        });

        let hits = index.search(&query).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 2);
    }

    #[test]
    fn exact_search_uses_indexed_in_and_range_filters() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        for chunk_id in 0..5 {
            let mut indexed_chunk = chunk(chunk_id, &format!("doc-{chunk_id}"), vec![1.0, 0.0]);
            indexed_chunk.metadata.insert(
                "source".to_owned(),
                MetadataValue::String(if chunk_id % 2 == 0 {
                    "notes".to_owned()
                } else {
                    "transcript".to_owned()
                }),
            );
            indexed_chunk
                .metadata
                .insert("stars".to_owned(), MetadataValue::Integer(chunk_id as i64));
            index.add_chunk(indexed_chunk).unwrap();
        }

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::All(vec![
            Filter::In {
                field: "source".to_owned(),
                values: vec![MetadataValue::String("notes".to_owned())],
            },
            Filter::Range {
                field: "stars".to_owned(),
                lower: Some(MetadataValue::Integer(2)),
                upper: Some(MetadataValue::Integer(4)),
            },
        ]));

        let hits = index.search(&query).unwrap();

        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![2, 4]
        );
    }

    #[test]
    fn exact_search_uses_indexed_not_equals_filter() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut hidden = chunk(1, "doc-1", vec![2.0, 0.0]);
        hidden
            .metadata
            .insert("archived".to_owned(), MetadataValue::Boolean(true));
        index.add_chunk(hidden).unwrap();

        let mut visible = chunk(2, "doc-2", vec![1.0, 0.0]);
        visible
            .metadata
            .insert("archived".to_owned(), MetadataValue::Boolean(false));
        index.add_chunk(visible).unwrap();

        let mut missing_field = chunk(3, "doc-3", vec![3.0, 0.0]);
        missing_field.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index.add_chunk(missing_field).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::NotEquals {
            field: "archived".to_owned(),
            value: MetadataValue::Boolean(true),
        });

        let hits = index.search(&query).unwrap();

        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![3, 2]
        );
    }

    #[test]
    fn filtered_search_excludes_upserted_old_metadata() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut old = chunk_input("old", vec![1.0, 0.0]);
        old.metadata
            .insert("source".to_owned(), MetadataValue::String("old".to_owned()));
        index.upsert_document(document("doc-1"), vec![old]).unwrap();

        let mut new = chunk_input("new", vec![0.0, 1.0]);
        new.metadata
            .insert("source".to_owned(), MetadataValue::String("new".to_owned()));
        index.upsert_document(document("doc-1"), vec![new]).unwrap();

        let old_query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("old".to_owned()),
        });
        let new_query = SearchQuery::new(vec![0.0, 1.0], 10).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("new".to_owned()),
        });

        assert!(index.search(&old_query).unwrap().is_empty());
        assert_eq!(index.search(&new_query).unwrap()[0].chunk_id, 1);
    }

    #[test]
    fn filtered_search_excludes_deleted_metadata() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut indexed_chunk = chunk_input("private", vec![1.0, 0.0]);
        indexed_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("private".to_owned()),
        );
        index
            .upsert_document(document("doc-1"), vec![indexed_chunk])
            .unwrap();
        index.delete_document("doc-1");

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("private".to_owned()),
        });

        assert!(index.search(&query).unwrap().is_empty());
    }

    #[test]
    fn cosine_search_normalizes_stored_vectors_and_queries() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index.add_chunk(chunk(1, "doc-1", vec![10.0, 0.0])).unwrap();
        index.add_chunk(chunk(2, "doc-2", vec![0.0, 2.0])).unwrap();

        let hits = index.search(&SearchQuery::new(vec![5.0, 0.0], 1)).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_close(hits[0].score, 1.0);
    }

    #[test]
    fn dot_product_search_keeps_raw_vector_magnitudes() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index.add_chunk(chunk(1, "doc-1", vec![10.0, 0.0])).unwrap();

        let hits = index.search(&SearchQuery::new(vec![5.0, 0.0], 1)).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_close(hits[0].score, 50.0);
    }

    #[test]
    fn exact_search_scores_f16_encoded_vectors() {
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::Cosine).with_vector_encoding(VectorEncoding::F16),
        )
        .unwrap();
        index.add_chunk(chunk(1, "doc-1", vec![10.0, 0.0])).unwrap();
        index.add_chunk(chunk(2, "doc-2", vec![0.0, 2.0])).unwrap();

        let hits = index.search(&SearchQuery::new(vec![5.0, 0.0], 1)).unwrap();

        assert_eq!(index.vector_encoding(), VectorEncoding::F16);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_close(hits[0].score, 1.0);
    }

    #[test]
    fn exact_search_scores_bf16_encoded_vectors() {
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::Cosine).with_vector_encoding(VectorEncoding::BF16),
        )
        .unwrap();
        index.add_chunk(chunk(1, "doc-1", vec![10.0, 0.0])).unwrap();
        index.add_chunk(chunk(2, "doc-2", vec![0.0, 2.0])).unwrap();

        let hits = index.search(&SearchQuery::new(vec![5.0, 0.0], 1)).unwrap();

        assert_eq!(index.vector_encoding(), VectorEncoding::BF16);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_close(hits[0].score, 1.0);
    }

    #[test]
    fn exact_search_scores_i8_scalar_quantized_vectors() {
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::Cosine)
                .with_vector_encoding(VectorEncoding::I8ScalarQuantized),
        )
        .unwrap();
        index.add_chunk(chunk(1, "doc-1", vec![10.0, 0.0])).unwrap();
        index.add_chunk(chunk(2, "doc-2", vec![0.0, 2.0])).unwrap();

        let hits = index.search(&SearchQuery::new(vec![5.0, 0.0], 1)).unwrap();

        assert_eq!(index.vector_encoding(), VectorEncoding::I8ScalarQuantized);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_close(hits[0].score, 1.0);
    }

    #[test]
    fn index_config_rejects_not_yet_supported_encodings() {
        let error = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::Cosine)
                .with_vector_encoding(VectorEncoding::BinaryQuantized),
        )
        .unwrap_err();

        assert_eq!(
            error,
            VectorKitError::UnsupportedVectorEncoding {
                encoding: "BinaryQuantized".to_owned()
            }
        );
    }

    #[test]
    fn keyword_search_returns_bm25_hits_with_matched_terms() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("Swift local search", vec![1.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-2"),
                vec![chunk_input("Rust vector core", vec![0.0, 1.0])],
            )
            .unwrap();

        let hits = index
            .keyword_search(&KeywordQuery::new("swift search", 10))
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 0);
        assert_eq!(hits[0].document_id, "doc-1");
        assert_eq!(hits[0].matched_terms, vec!["search", "swift"]);
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn keyword_search_excludes_superseded_chunks() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("old codename alpha", vec![1.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("new codename beta", vec![0.0, 1.0])],
            )
            .unwrap();

        let alpha_hits = index
            .keyword_search(&KeywordQuery::new("alpha", 10))
            .unwrap();
        let beta_hits = index
            .keyword_search(&KeywordQuery::new("beta", 10))
            .unwrap();

        assert!(alpha_hits.is_empty());
        assert_eq!(beta_hits.len(), 1);
        assert_eq!(beta_hits[0].chunk_id, 1);
    }

    #[test]
    fn keyword_search_excludes_deleted_documents() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("private exact phrase", vec![1.0, 0.0])],
            )
            .unwrap();

        assert_eq!(index.delete_document("doc-1"), 1);

        let hits = index
            .keyword_search(&KeywordQuery::new("exact phrase", 10))
            .unwrap();

        assert!(hits.is_empty());
    }

    #[test]
    fn keyword_search_applies_metadata_filters_before_top_k() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut notes_document = document("doc-1");
        notes_document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index
            .upsert_document(
                notes_document,
                vec![chunk_input("shared rare token", vec![1.0, 0.0])],
            )
            .unwrap();

        let mut transcript_document = document("doc-2");
        transcript_document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index
            .upsert_document(
                transcript_document,
                vec![chunk_input("shared rare token", vec![0.0, 1.0])],
            )
            .unwrap();

        let query = KeywordQuery::new("shared token", 1).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("transcript".to_owned()),
        });

        let hits = index.keyword_search(&query).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document_id, "doc-2");
    }

    #[test]
    fn upsert_document_assigns_internal_chunk_ids_and_version() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);

        let chunk_ids = index
            .upsert_document(
                document("doc-1"),
                vec![
                    chunk_input("first", vec![1.0, 0.0]),
                    chunk_input("second", vec![0.0, 1.0]),
                ],
            )
            .unwrap();

        assert_eq!(chunk_ids, vec![0, 1]);
        assert_eq!(index.chunk(0).unwrap().version, 1);
        assert_eq!(index.chunk(1).unwrap().version, 1);
    }

    #[test]
    fn chunk_lookup_returns_none_for_missing_internal_id() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index.add_chunk(chunk(10, "doc-1", vec![1.0, 0.0])).unwrap();

        assert!(index.chunk(9).is_none());
        assert!(index.chunk(11).is_none());
    }

    #[test]
    fn chunk_lookup_supports_sparse_manual_chunk_ids() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index.add_chunk(chunk(10, "doc-1", vec![1.0, 0.0])).unwrap();

        let found = index.chunk(10).unwrap();

        assert_eq!(found.chunk_id, 10);
        assert_eq!(found.document_id, "doc-1");
    }

    #[test]
    fn chunk_lookup_returns_tombstoned_chunks_for_debug_access() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(document("doc-1"), vec![chunk_input("old", vec![1.0, 0.0])])
            .unwrap();
        index
            .upsert_document(document("doc-1"), vec![chunk_input("new", vec![0.0, 1.0])])
            .unwrap();

        let old_chunk = index.chunk(0).unwrap();
        let new_chunk = index.chunk(1).unwrap();

        assert!(old_chunk.deleted);
        assert!(!new_chunk.deleted);
    }

    #[test]
    fn manual_add_chunk_advances_next_internal_chunk_id() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index.add_chunk(chunk(10, "doc-1", vec![1.0, 0.0])).unwrap();

        let chunk_ids = index
            .upsert_document(
                document("doc-2"),
                vec![chunk_input("first", vec![0.0, 1.0])],
            )
            .unwrap();

        assert_eq!(chunk_ids, vec![11]);
    }

    #[test]
    fn upsert_document_marks_old_chunks_deleted() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let old_chunk_ids = index
            .upsert_document(document("doc-1"), vec![chunk_input("old", vec![1.0, 0.0])])
            .unwrap();

        let new_chunk_ids = index
            .upsert_document(document("doc-1"), vec![chunk_input("new", vec![2.0, 0.0])])
            .unwrap();

        assert_eq!(old_chunk_ids, vec![0]);
        assert_eq!(new_chunk_ids, vec![1]);
        assert!(index.chunk(0).unwrap().deleted);
        assert_eq!(index.chunk(1).unwrap().version, 2);

        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
    }

    #[test]
    fn upsert_document_with_zero_chunks_tombstones_old_chunks() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(document("doc-1"), vec![chunk_input("old", vec![1.0, 0.0])])
            .unwrap();

        let new_chunk_ids = index
            .upsert_document(document("doc-1"), Vec::new())
            .unwrap();
        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert!(new_chunk_ids.is_empty());
        assert_eq!(index.len(), 1);
        assert_eq!(index.active_chunk_count(), 0);
        assert!(index.chunk(0).unwrap().deleted);
        assert!(hits.is_empty());
    }

    #[test]
    fn delete_document_removes_active_chunks_from_search() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![
                    chunk_input("first", vec![1.0, 0.0]),
                    chunk_input("second", vec![0.0, 1.0]),
                ],
            )
            .unwrap();

        let deleted_count = index.delete_document("doc-1");
        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(deleted_count, 2);
        assert!(hits.is_empty());
    }

    #[test]
    fn delete_document_is_idempotent_for_unknown_or_deleted_documents() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);

        assert_eq!(index.delete_document("missing-doc"), 0);

        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("first", vec![1.0, 0.0])],
            )
            .unwrap();

        assert_eq!(index.delete_document("doc-1"), 1);
        assert_eq!(index.delete_document("doc-1"), 0);
        assert_eq!(index.active_chunk_count(), 0);
    }

    #[test]
    fn upsert_document_validates_all_chunks_before_mutating_existing_document() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(document("doc-1"), vec![chunk_input("old", vec![1.0, 0.0])])
            .unwrap();

        let error = index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("invalid", vec![1.0, 0.0, 0.0])],
            )
            .unwrap_err();
        let hits = index.search(&SearchQuery::new(vec![1.0, 0.0], 10)).unwrap();

        assert_eq!(
            error,
            VectorKitError::InvalidDimension {
                expected: 2,
                actual: 3
            }
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 0);
        assert!(!index.chunk(0).unwrap().deleted);
    }

    #[test]
    fn upsert_document_merges_document_and_chunk_metadata_for_filters() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut document = document("doc-1");
        document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );

        let mut chunk = chunk_input("first", vec![1.0, 0.0]);
        chunk.metadata.insert(
            "section".to_owned(),
            MetadataValue::String("intro".to_owned()),
        );

        index.upsert_document(document, vec![chunk]).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::All(vec![
            Filter::Equals {
                field: "source".to_owned(),
                value: MetadataValue::String("notes".to_owned()),
            },
            Filter::Equals {
                field: "section".to_owned(),
                value: MetadataValue::String("intro".to_owned()),
            },
        ]));

        let hits = index.search(&query).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 0);
    }

    #[test]
    fn chunk_metadata_overrides_document_metadata() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut document = document("doc-1");
        document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("document".to_owned()),
        );

        let mut chunk = chunk_input("first", vec![1.0, 0.0]);
        chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("chunk".to_owned()),
        );

        index.upsert_document(document, vec![chunk]).unwrap();

        let hit_chunk = index.chunk(0).unwrap();

        assert_eq!(
            hit_chunk.metadata.get("source"),
            Some(&MetadataValue::String("chunk".to_owned()))
        );
    }
}
