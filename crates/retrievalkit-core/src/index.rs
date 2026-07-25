use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bm25::{Bm25Config, Bm25Index, PersistedBm25Index};
use crate::candidate_scope::CandidateScope;
use crate::corpus_index::CorpusIndex;
use crate::error::{Result, RetrievalKitError};
use crate::filter::Filter;
use crate::metadata::{estimated_metadata_payload_bytes, Metadata, MetadataValue};
use crate::metadata_index::MetadataFilterIndex;
use crate::record_store::{
    ChunkIdentity, ChunkKey, CorpusId, FieldName, GenerationId, Record, RecordId, RecordStore,
    RecordType, RecordValue,
};
use crate::retrieval_index::{RetrievalConfiguration, RetrievalIndex};
use crate::scoring::{self, EncodedVectorStore};
use crate::types::{
    Chunk, ChunkId, ChunkInput, CompactionReport, Document, HybridFusion, HybridFusionTrace,
    HybridHit, HybridQuery, HybridTrace, IndexConfig, IndexFileSizeReport, IndexPersistenceOptions,
    IndexSizeEstimate, KeywordHit, KeywordQuery, RecordChunkInput, SearchHit, SearchQuery,
    SearchTrace, StoredChunk, VectorEncoding, VectorMetric,
};

const FORMAT_VERSION: u32 = 4;
const CHECKSUM_FORMAT_VERSION: u32 = 3;
const TRANSACTIONAL_FORMAT_VERSION: u32 = 2;
const LEGACY_FORMAT_VERSION: u32 = 1;
const CREATED_WITH: &str = "retrievalkit";
const MANIFEST_FILE: &str = "manifest.json";
const SAVE_LOCK_FILE: &str = ".save.lock";
const SNAPSHOTS_DIRECTORY: &str = ".snapshots";
const VECTORS_FILE: &str = "vectors.vec";
const CHUNKS_FILE: &str = "chunks.bin";
const RECORDS_FILE: &str = "records.bin";
const BM25_FILE: &str = "bm25.bin";
const TOMBSTONES_FILE: &str = "tombstones.bin";
const PERSISTENCE_COMPRESSION: FileCompression = FileCompression::Zstd;
const ZSTD_COMPRESSION_LEVEL: i32 = 3;
const CHUNKS_MAGIC: &[u8; 4] = b"VKCH";
const CHUNKS_FORMAT_VERSION: u32 = 2;
const LEGACY_CHUNKS_FORMAT_VERSION: u32 = 1;
const METADATA_STRING: u8 = 0;
const METADATA_INTEGER: u8 = 1;
const METADATA_FLOAT: u8 = 2;
const METADATA_BOOLEAN: u8 = 3;
const METADATA_TIMESTAMP_MILLIS: u8 = 4;
static SNAPSHOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn default_corpus_id() -> CorpusId {
    CorpusId::new("default").expect("the built-in corpus ID is valid")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveCheckpoint {
    VectorsWritten,
    ChunksWritten,
    RecordsWritten,
    Bm25Written,
    TombstonesWritten,
    SnapshotSynced,
    ManifestWritten,
}

struct SaveLock {
    file: fs::File,
}

impl SaveLock {
    fn acquire(directory: &Path) -> Result<Self> {
        let path = directory.join(SAVE_LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| persistence_error("open save lock", &path, &error))?;
        file.try_lock_exclusive().map_err(|error| {
            persistence_error(
                "acquire exclusive save lock because another save may already be running",
                &path,
                &error,
            )
        })?;
        Ok(Self { file })
    }
}

impl Drop for SaveLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

#[derive(Debug, Clone)]
pub struct ExactVectorIndex {
    corpus: CorpusIndex,
    retrieval: RetrievalIndex,
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

    /// Creates an empty index in an explicit stable corpus namespace.
    pub fn try_with_config_in_corpus(config: IndexConfig, corpus_id: CorpusId) -> Result<Self> {
        Self::from_parts_in_corpus(config, Bm25Config::default(), corpus_id)
    }

    /// Creates an empty retrieval facade with an explicit capability mode.
    pub fn try_with_retrieval_configuration_in_corpus(
        configuration: RetrievalConfiguration,
        corpus_id: CorpusId,
    ) -> Result<Self> {
        Ok(Self {
            corpus: CorpusIndex::new(corpus_id),
            retrieval: RetrievalIndex::new(configuration)?,
        })
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
        Self::from_parts(
            dimension,
            metric,
            VectorEncoding::I8ScalarQuantized,
            bm25_config,
        )
        .expect("I8 scalar-quantized vector encoding is supported")
    }

    fn from_parts(
        dimension: usize,
        metric: VectorMetric,
        vector_encoding: VectorEncoding,
        bm25_config: Bm25Config,
    ) -> Result<Self> {
        Self::from_parts_in_corpus(
            IndexConfig {
                dimension,
                metric,
                vector_encoding,
            },
            bm25_config,
            default_corpus_id(),
        )
    }

    fn from_parts_in_corpus(
        config: IndexConfig,
        bm25_config: Bm25Config,
        corpus_id: CorpusId,
    ) -> Result<Self> {
        Self::try_with_retrieval_configuration_in_corpus(
            RetrievalConfiguration::semantic(config).with_hybrid_configuration(
                crate::retrieval_index::HybridRetrievalConfiguration { bm25: bm25_config },
            ),
            corpus_id,
        )
    }

    /// Returns the required embedding dimension for indexed chunks and queries.
    pub fn dimension(&self) -> usize {
        self.retrieval.dimension
    }

    /// Returns the vector metric used for scoring.
    pub fn metric(&self) -> VectorMetric {
        self.retrieval.metric
    }

    /// Returns the stored vector representation used by this index.
    pub fn vector_encoding(&self) -> VectorEncoding {
        self.retrieval.vector_encoding
    }

    pub fn retrieval(&self) -> &RetrievalIndex {
        &self.retrieval
    }

    /// Returns the stable namespace this in-memory generation belongs to.
    pub fn corpus_id(&self) -> &CorpusId {
        self.corpus.corpus_id()
    }

    /// Returns the generation used to validate external candidate scopes.
    pub fn generation(&self) -> GenerationId {
        self.corpus.generation()
    }

    /// Returns the canonical graph-neutral corpus behind this retrieval facade.
    pub fn corpus(&self) -> &CorpusIndex {
        &self.corpus
    }

    /// Returns the canonical graph-neutral records behind derived retrieval data.
    pub fn record_store(&self) -> &RecordStore {
        self.corpus.record_store()
    }

    pub fn record(&self, record_id: &RecordId) -> Option<&Record> {
        self.corpus.record(record_id)
    }

    pub fn hydrate_records<'a>(&'a self, record_ids: &[RecordId]) -> Vec<Option<&'a Record>> {
        self.corpus.hydrate_records(record_ids)
    }

    /// Resolves a stable external chunk identity in the active generation.
    pub fn chunk_id_for_identity(&self, identity: &ChunkIdentity) -> Option<ChunkId> {
        self.corpus.chunk_id_for_identity(identity)
    }

    pub fn chunk_identity(&self, chunk_id: ChunkId) -> Option<&ChunkIdentity> {
        self.corpus.chunk_identity(chunk_id)
    }

    /// Iterates active external/internal chunk mappings in stable identity order.
    pub fn chunk_identities(&self) -> impl Iterator<Item = (&ChunkIdentity, ChunkId)> {
        self.corpus.chunk_identities()
    }

    /// Returns the total number of stored chunks, including tombstoned chunks.
    pub fn len(&self) -> usize {
        self.corpus.len()
    }

    /// Returns true when no chunks have been stored.
    pub fn is_empty(&self) -> bool {
        self.corpus.is_empty()
    }

    /// Returns the number of chunks currently eligible for search results.
    pub fn active_chunk_count(&self) -> usize {
        self.corpus.active_chunk_count()
    }

    /// Returns the number of stored chunks currently marked deleted.
    pub fn tombstoned_chunk_count(&self) -> usize {
        self.corpus.tombstoned_chunk_count()
    }

    /// Returns an approximate payload byte breakdown for the currently loaded index.
    pub fn size_estimate(&self) -> IndexSizeEstimate {
        IndexSizeEstimate {
            vector_bytes: self.retrieval.encoded_vectors.estimated_payload_bytes(),
            chunk_record_bytes: self.corpus.chunks.len() * std::mem::size_of::<ChunkId>(),
            document_id_bytes: self
                .corpus
                .chunks
                .iter()
                .map(|chunk| chunk.document_id.len())
                .sum(),
            text_bytes: self
                .corpus
                .chunks
                .iter()
                .map(|chunk| chunk.text.len())
                .sum(),
            metadata_bytes: self
                .corpus
                .chunks
                .iter()
                .map(|chunk| estimated_metadata_payload_bytes(&chunk.metadata))
                .sum(),
            tombstone_bytes: self.corpus.chunks.len() * std::mem::size_of::<bool>(),
            version_bytes: self.corpus.chunks.len() * std::mem::size_of::<u64>(),
            chunk_offset_bytes: self.corpus.chunk_offsets.len()
                * std::mem::size_of::<Option<usize>>()
                + self.corpus.active_offsets.len() * std::mem::size_of::<usize>(),
            bm25_bytes: self
                .retrieval
                .bm25
                .as_ref()
                .map_or(0, Bm25Index::estimated_payload_bytes),
            metadata_filter_bytes: self
                .retrieval
                .metadata_filter_index
                .estimated_payload_bytes(),
            record_store_bytes: self.corpus.record_store.estimated_payload_bytes(),
            chunk_identity_bytes: self
                .corpus
                .chunk_ids_by_identity
                .keys()
                .map(|identity| {
                    identity.record_id.as_str().len()
                        + identity.chunk_key.as_str().len()
                        + std::mem::size_of::<ChunkId>()
                })
                .sum::<usize>()
                .saturating_mul(2),
        }
    }

    /// Saves the loaded index to a local directory and returns actual file sizes.
    pub fn save_to_dir(&self, directory: impl AsRef<Path>) -> Result<IndexFileSizeReport> {
        self.save_to_dir_with_options(directory, IndexPersistenceOptions::default())
    }

    /// Saves the loaded index with explicit persistence options.
    pub fn save_to_dir_with_options(
        &self,
        directory: impl AsRef<Path>,
        options: IndexPersistenceOptions,
    ) -> Result<IndexFileSizeReport> {
        self.save_to_dir_with_checkpoints(directory, options, |_| Ok(()))
    }

    fn save_to_dir_with_checkpoints(
        &self,
        directory: impl AsRef<Path>,
        options: IndexPersistenceOptions,
        mut checkpoint: impl FnMut(SaveCheckpoint) -> Result<()>,
    ) -> Result<IndexFileSizeReport> {
        self.validate_record_state()?;
        let directory = directory.as_ref();
        fs::create_dir_all(directory)
            .map_err(|error| persistence_error("create directory", directory, &error))?;
        let _save_lock = SaveLock::acquire(directory)?;

        let snapshots_directory = directory.join(SNAPSHOTS_DIRECTORY);
        fs::create_dir_all(&snapshots_directory).map_err(|error| {
            persistence_error("create snapshots directory", &snapshots_directory, &error)
        })?;
        let snapshot_id = next_snapshot_id()?;
        let snapshot_directory = snapshots_directory.join(&snapshot_id);
        fs::create_dir(&snapshot_directory)
            .map_err(|error| persistence_error("create snapshot", &snapshot_directory, &error))?;

        let vectors_path = snapshot_directory.join(VECTORS_FILE);
        let chunks_path = snapshot_directory.join(CHUNKS_FILE);
        let records_path = snapshot_directory.join(RECORDS_FILE);
        let bm25_path = snapshot_directory.join(BM25_FILE);
        let tombstones_path = snapshot_directory.join(TOMBSTONES_FILE);
        let manifest_path = directory.join(MANIFEST_FILE);
        let manifest_tmp_path = directory.join(format!("manifest.{snapshot_id}.tmp"));

        write_file(
            &vectors_path,
            &self.retrieval.encoded_vectors.to_payload_bytes(),
        )?;
        checkpoint(SaveCheckpoint::VectorsWritten)?;
        let chunk_payload = encode_chunks(&self.corpus.chunks)?;
        let chunk_uncompressed_bytes =
            checked_usize_to_u64(chunk_payload.len(), "chunk payload byte count")?;
        write_file(
            &chunks_path,
            &compress_payload(&chunks_path, &chunk_payload, PERSISTENCE_COMPRESSION)?,
        )?;
        checkpoint(SaveCheckpoint::ChunksWritten)?;
        let record_payload = serde_json::to_vec(&PersistedRecordState {
            record_store: self.corpus.record_store.clone(),
            chunk_identities: self
                .corpus
                .chunk_ids_by_identity
                .iter()
                .map(|(identity, chunk_id)| (identity.clone(), *chunk_id))
                .collect(),
        })
        .map_err(|error| RetrievalKitError::InvalidFormat {
            message: format!("could not encode canonical records: {error}"),
        })?;
        let records_uncompressed_bytes =
            checked_usize_to_u64(record_payload.len(), "record payload byte count")?;
        write_file(
            &records_path,
            &compress_payload(&records_path, &record_payload, PERSISTENCE_COMPRESSION)?,
        )?;
        checkpoint(SaveCheckpoint::RecordsWritten)?;
        let include_bm25 = options.include_bm25 && self.retrieval.has_bm25();
        let mut bm25_uncompressed_bytes = 0;
        if include_bm25 {
            let bm25_payload = self
                .retrieval
                .require_bm25()?
                .to_persisted()
                .to_payload_bytes()?;
            bm25_uncompressed_bytes =
                checked_usize_to_u64(bm25_payload.len(), "bm25 payload byte count")?;
            write_file(
                &bm25_path,
                &compress_payload(&bm25_path, &bm25_payload, PERSISTENCE_COMPRESSION)?,
            )?;
        }
        checkpoint(SaveCheckpoint::Bm25Written)?;
        write_file(
            &tombstones_path,
            &self
                .corpus
                .chunks
                .iter()
                .map(|chunk| u8::from(chunk.deleted))
                .collect::<Vec<_>>(),
        )?;
        checkpoint(SaveCheckpoint::TombstonesWritten)?;

        let manifest = PersistedManifest {
            format_version: FORMAT_VERSION,
            snapshot_id: Some(snapshot_id.clone()),
            created_with: CREATED_WITH.to_owned(),
            corpus_id: self.corpus.corpus_id.clone(),
            generation: self.corpus.generation,
            dimension: self.retrieval.dimension,
            metric: self.retrieval.metric,
            vector_count: self.corpus.chunks.len(),
            active_chunk_count: self.active_chunk_count(),
            retrieval_mode: self.retrieval.mode(),
            has_bm25: include_bm25,
            has_records: true,
            vector_encoding: self.retrieval.vector_encoding,
            vector_bytes: self.retrieval.encoded_vectors.estimated_payload_bytes(),
            chunk_bytes: file_size(&chunks_path)?,
            chunk_uncompressed_bytes,
            chunk_compression: PERSISTENCE_COMPRESSION,
            records_bytes: file_size(&records_path)?,
            records_uncompressed_bytes,
            records_compression: PERSISTENCE_COMPRESSION,
            bm25_bytes: if include_bm25 {
                file_size(&bm25_path)?
            } else {
                0
            },
            bm25_uncompressed_bytes,
            bm25_compression: if include_bm25 {
                PERSISTENCE_COMPRESSION
            } else {
                FileCompression::None
            },
            tombstone_bytes: file_size(&tombstones_path)?,
            checksums: Some(PersistedChecksums {
                algorithm: ChecksumAlgorithm::Sha256,
                vectors: sha256_file(&vectors_path)?,
                chunks: sha256_file(&chunks_path)?,
                records: Some(sha256_file(&records_path)?),
                bm25: if include_bm25 {
                    Some(sha256_file(&bm25_path)?)
                } else {
                    None
                },
                tombstones: sha256_file(&tombstones_path)?,
            }),
            normalization: match self.retrieval.metric {
                VectorMetric::Cosine => "unit_l2",
                VectorMetric::DotProduct => "none",
            }
            .to_owned(),
        };

        manifest.validate()?;
        validate_snapshot_file_sizes(&manifest, &snapshot_directory)?;
        validate_snapshot_checksums(&manifest, &snapshot_directory)?;
        sync_directory(&snapshot_directory)?;
        sync_directory(&snapshots_directory)?;
        checkpoint(SaveCheckpoint::SnapshotSynced)?;
        write_json_file(&manifest_tmp_path, &manifest)?;
        checkpoint(SaveCheckpoint::ManifestWritten)?;
        fs::rename(&manifest_tmp_path, &manifest_path)
            .map_err(|error| persistence_error("publish manifest", &manifest_path, &error))?;
        sync_directory(directory)?;

        cleanup_unreferenced_snapshots(&snapshots_directory, &snapshot_id);
        cleanup_legacy_files(directory);
        cleanup_temporary_manifests(directory);

        Self::persisted_file_sizes(directory)
    }

    /// Loads a previously saved local index directory.
    pub fn load_from_dir(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref();
        let manifest_path = directory.join(MANIFEST_FILE);
        let manifest: PersistedManifest = read_json_file(&manifest_path)?;
        manifest.validate()?;

        let data_directory = manifest.data_directory(directory)?;
        let vectors_path = data_directory.join(VECTORS_FILE);
        let chunks_path = data_directory.join(CHUNKS_FILE);
        let records_path = data_directory.join(RECORDS_FILE);
        let bm25_path = data_directory.join(BM25_FILE);
        let tombstones_path = data_directory.join(TOMBSTONES_FILE);

        validate_snapshot_file_sizes(&manifest, &data_directory)?;
        validate_snapshot_checksums(&manifest, &data_directory)?;
        let vector_bytes = read_file(&vectors_path)?;
        if vector_bytes.len() != manifest.vector_bytes {
            return Err(RetrievalKitError::InvalidFormat {
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
        let chunk_payload = read_payload_file(
            &chunks_path,
            manifest.chunk_compression,
            manifest.chunk_uncompressed_bytes,
        )?;
        let chunks = decode_chunks(&chunk_payload)?;
        if chunks.len() != manifest.vector_count {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!(
                    "manifest vector count {} does not match chunk count {}",
                    manifest.vector_count,
                    chunks.len()
                ),
            });
        }

        let tombstones = read_file(&tombstones_path)?;
        if tombstones.len() != chunks.len() {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!(
                    "tombstone count {} does not match chunk count {}",
                    tombstones.len(),
                    chunks.len()
                ),
            });
        }
        for (offset, (chunk, tombstone)) in chunks.iter().zip(&tombstones).enumerate() {
            if *tombstone > 1 {
                return Err(RetrievalKitError::CorruptIndex {
                    path: tombstones_path.display().to_string(),
                    message: format!(
                        "invalid tombstone byte {tombstone} at offset {offset}; expected 0 or 1"
                    ),
                });
            }
            if chunk.deleted != (*tombstone != 0) {
                return Err(RetrievalKitError::CorruptIndex {
                    path: tombstones_path.display().to_string(),
                    message: format!("chunk {offset} tombstone does not match chunk record"),
                });
            }
        }
        let persisted_bm25 = if manifest.has_bm25 {
            let bm25_payload = read_payload_file(
                &bm25_path,
                manifest.bm25_compression,
                manifest.bm25_uncompressed_bytes,
            )?;
            let persisted_bm25 = PersistedBm25Index::from_payload_bytes(&bm25_payload)?;
            validate_bm25_state_matches_chunks(&persisted_bm25, &chunks)?;
            Some(persisted_bm25)
        } else {
            None
        };
        let persisted_records = if manifest.has_records {
            let record_payload = read_payload_file(
                &records_path,
                manifest.records_compression,
                manifest.records_uncompressed_bytes,
            )?;
            Some(
                serde_json::from_slice::<PersistedRecordState>(&record_payload).map_err(
                    |error| RetrievalKitError::InvalidFormat {
                        message: format!("could not decode canonical records: {error}"),
                    },
                )?,
            )
        } else {
            None
        };

        let vector = IndexConfig {
            dimension: manifest.dimension,
            metric: manifest.metric,
            vector_encoding: manifest.vector_encoding,
        };
        let configuration = RetrievalConfiguration::semantic(vector);
        let mut index = Self::try_with_retrieval_configuration_in_corpus(
            configuration,
            manifest.corpus_id.clone(),
        )?;
        index.corpus.generation = manifest.generation;
        index.retrieval.encoded_vectors = encoded_vectors;
        index.corpus.chunks = chunks;
        if let Some(persisted_records) = persisted_records {
            index.corpus.record_store = persisted_records.record_store;
            for (identity, chunk_id) in persisted_records.chunk_identities {
                if index
                    .corpus
                    .chunk_ids_by_identity
                    .insert(identity.clone(), chunk_id)
                    .is_some()
                {
                    return Err(RetrievalKitError::InvalidFormat {
                        message: format!(
                            "canonical record payload repeats chunk identity {}/{}",
                            identity.record_id.as_str(),
                            identity.chunk_key.as_str()
                        ),
                    });
                }
            }
            index.corpus.chunk_identities = index
                .corpus
                .chunk_ids_by_identity
                .iter()
                .map(|(identity, chunk_id)| (*chunk_id, identity.clone()))
                .collect();
        }
        let rebuild_bm25 = persisted_bm25.is_none();
        if let Some(persisted_bm25) = persisted_bm25 {
            index.retrieval.bm25 = Some(Bm25Index::from_persisted(
                Bm25Config::default(),
                persisted_bm25,
            )?);
        }
        index.rebuild_derived_state_from_loaded_chunks();
        if rebuild_bm25 {
            index.rebuild_bm25_from_loaded_chunks();
        }
        index.validate_record_state()?;

        if index.active_chunk_count() != manifest.active_chunk_count {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!(
                    "manifest active chunk count {} does not match loaded active chunk count {}",
                    manifest.active_chunk_count,
                    index.active_chunk_count()
                ),
            });
        }

        Ok(index)
    }

    /// Verifies a saved index without modifying it.
    ///
    /// Validation covers the manifest, file sizes, checksums when present, all
    /// persisted payloads, tombstone values, and cross-file consistency.
    pub fn validate_dir(directory: impl AsRef<Path>) -> Result<()> {
        Self::load_from_dir(directory).map(|_| ())
    }

    /// Returns actual file sizes for a saved index directory.
    pub fn persisted_file_sizes(directory: impl AsRef<Path>) -> Result<IndexFileSizeReport> {
        let directory = directory.as_ref();
        let manifest: PersistedManifest = read_json_file(&directory.join(MANIFEST_FILE))?;
        manifest.validate()?;
        let data_directory = manifest.data_directory(directory)?;
        Ok(IndexFileSizeReport {
            manifest_bytes: file_size(&directory.join(MANIFEST_FILE))?,
            vectors_bytes: file_size(&data_directory.join(VECTORS_FILE))?,
            chunks_bytes: file_size(&data_directory.join(CHUNKS_FILE))?,
            records_bytes: file_size_if_exists(&data_directory.join(RECORDS_FILE))?,
            bm25_bytes: file_size_if_exists(&data_directory.join(BM25_FILE))?,
            tombstones_bytes: file_size(&data_directory.join(TOMBSTONES_FILE))?,
        })
    }

    /// Adds a prebuilt chunk directly.
    ///
    /// Most callers should use `upsert_document` so RetrievalKit can assign
    /// internal chunk IDs and enforce document version tombstones. This method
    /// remains useful for tests and future persistence-loading paths.
    pub fn add_chunk(&mut self, chunk: Chunk) -> Result<()> {
        self.validate_dimension(chunk.embedding.len())?;
        let offset = self.corpus.chunks.len();
        self.corpus.next_chunk_id = self
            .corpus
            .next_chunk_id
            .max(chunk.chunk_id.saturating_add(1));
        self.corpus
            .record_versions
            .entry(chunk.document_id.clone())
            .and_modify(|version| *version = (*version).max(chunk.version))
            .or_insert(chunk.version);
        if let Some(bm25) = &mut self.retrieval.bm25 {
            bm25.add_chunk(chunk.chunk_id, &chunk.text, !chunk.deleted);
        }
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
            self.corpus.active_offsets.push(offset);
            self.retrieval
                .metadata_filter_index
                .insert(offset, &stored_chunk.metadata);
        }
        self.corpus.chunks.push(stored_chunk);
        self.corpus.generation = self.corpus.generation.next();
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
        let record_id = RecordId::new(document.id.clone())?;
        let fields = document
            .metadata
            .iter()
            .map(|(field, value)| {
                Ok((
                    FieldName::new(field.clone())?,
                    record_value_from_metadata(value),
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let record = Record {
            id: record_id,
            record_type: RecordType::new("Document")?,
            fields,
            content: Some(document.text),
        };
        let record_chunks = chunk_inputs
            .into_iter()
            .enumerate()
            .map(|(ordinal, chunk)| {
                Ok(RecordChunkInput {
                    key: ChunkKey::new(format!("ordinal-{ordinal}"))?,
                    text: chunk.text,
                    embedding: chunk.embedding,
                    metadata: chunk.metadata,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.upsert_record(record, document.metadata, record_chunks)
    }

    /// Adds or replaces one canonical record and all of its derived chunks.
    pub fn upsert_record(
        &mut self,
        record: Record,
        projected_metadata: Metadata,
        chunk_inputs: Vec<RecordChunkInput>,
    ) -> Result<Vec<ChunkId>> {
        record.validate()?;
        for chunk in &chunk_inputs {
            self.validate_dimension(chunk.embedding.len())?;
        }
        let mut keys = BTreeSet::new();
        for chunk in &chunk_inputs {
            if !keys.insert(chunk.key.clone()) {
                return Err(RetrievalKitError::InvalidIdentity {
                    kind: "ChunkKey",
                    value: chunk.key.as_str().to_owned(),
                    message: "must be unique within one record generation".to_owned(),
                });
            }
        }
        self.retrieval
            .encoded_vectors
            .reserve_rows(chunk_inputs.len(), self.retrieval.dimension);

        let document_id = record.id.as_str().to_owned();
        let version = self
            .corpus
            .record_versions
            .get(&document_id)
            .copied()
            .unwrap_or(0)
            + 1;

        let mut deactivated_offsets = Vec::new();
        for (offset, chunk) in self.corpus.chunks.iter_mut().enumerate() {
            if chunk.document_id == document_id {
                if !chunk.deleted {
                    self.retrieval
                        .metadata_filter_index
                        .remove(offset, &chunk.metadata);
                    deactivated_offsets.push(offset);
                }
                chunk.deleted = true;
                if let Some(bm25) = &mut self.retrieval.bm25 {
                    bm25.deactivate_chunk(chunk.chunk_id);
                }
            }
        }
        self.remove_active_offsets(&deactivated_offsets);
        self.remove_chunk_identities_for_record(&record.id);

        let mut chunk_ids = Vec::with_capacity(chunk_inputs.len());
        for chunk_input in chunk_inputs {
            let offset = self.corpus.chunks.len();
            let chunk_id = self.allocate_chunk_id();
            chunk_ids.push(chunk_id);
            if let Some(bm25) = &mut self.retrieval.bm25 {
                bm25.add_chunk(chunk_id, &chunk_input.text, true);
            }
            self.register_chunk_offset(chunk_id, offset);
            self.push_embedding(&chunk_input.embedding);
            let stored_chunk = StoredChunk {
                chunk_id,
                document_id: document_id.clone(),
                text: chunk_input.text,
                metadata: merge_metadata(&projected_metadata, chunk_input.metadata),
                deleted: false,
                version,
            };
            let identity = ChunkIdentity::new(record.id.clone(), chunk_input.key);
            self.corpus
                .chunk_ids_by_identity
                .insert(identity.clone(), chunk_id);
            self.corpus.chunk_identities.insert(chunk_id, identity);
            self.corpus.active_offsets.push(offset);
            self.retrieval
                .metadata_filter_index
                .insert(offset, &stored_chunk.metadata);
            self.corpus.chunks.push(stored_chunk);
        }

        self.corpus.record_versions.insert(document_id, version);
        self.corpus.record_store.upsert(record)?;
        self.corpus.generation = self.corpus.generation.next();

        Ok(chunk_ids)
    }

    /// Tombstones all active chunks for a caller-owned document ID.
    ///
    /// Returns the number of chunks newly marked deleted. Repeated deletes are
    /// idempotent and return zero once no active chunks remain.
    pub fn delete_document(&mut self, document_id: &str) -> usize {
        let record_id = RecordId::new(document_id.to_owned()).ok();
        self.delete_record_by_id(document_id, record_id.as_ref())
    }

    pub fn delete_record(&mut self, record_id: &RecordId) -> usize {
        self.delete_record_by_id(record_id.as_str(), Some(record_id))
    }

    fn delete_record_by_id(&mut self, document_id: &str, record_id: Option<&RecordId>) -> usize {
        let mut deleted_count = 0;
        let mut deactivated_offsets = Vec::new();
        for (offset, chunk) in self.corpus.chunks.iter_mut().enumerate() {
            if chunk.document_id == document_id && !chunk.deleted {
                self.retrieval
                    .metadata_filter_index
                    .remove(offset, &chunk.metadata);
                chunk.deleted = true;
                if let Some(bm25) = &mut self.retrieval.bm25 {
                    bm25.deactivate_chunk(chunk.chunk_id);
                }
                deactivated_offsets.push(offset);
                deleted_count += 1;
            }
        }
        self.remove_active_offsets(&deactivated_offsets);
        let removed_record = record_id.and_then(|id| self.corpus.record_store.delete(id));
        if let Some(record_id) = record_id {
            self.remove_chunk_identities_for_record(record_id);
        }
        if deleted_count > 0 || removed_record.is_some() {
            self.corpus.generation = self.corpus.generation.next();
        }
        deleted_count
    }

    /// Rebuilds in-memory storage without tombstoned chunks.
    ///
    /// Active chunk IDs, document versions, and the next chunk ID are preserved.
    /// Deleted chunk IDs stop resolving through `chunk` and are never reused.
    /// Replacement structures are fully built before the index is mutated.
    pub fn compact(&mut self) -> Result<CompactionReport> {
        let chunks_before = self.corpus.chunks.len();
        let estimated_bytes_before = self.size_estimate().total_bytes();
        if self.tombstoned_chunk_count() == 0 {
            return Ok(CompactionReport {
                chunks_before,
                chunks_after: chunks_before,
                chunks_removed: 0,
                estimated_bytes_before,
                estimated_bytes_after: estimated_bytes_before,
                estimated_bytes_reclaimed: 0,
            });
        }
        let active_offsets = self.corpus.active_offsets.clone();

        let new_vectors = self
            .retrieval
            .encoded_vectors
            .select_rows(&active_offsets, self.retrieval.dimension)?;
        let new_chunks = active_offsets
            .iter()
            .map(|&offset| {
                self.corpus.chunks
                    .get(offset)
                    .cloned()
                    .ok_or_else(|| RetrievalKitError::InvalidFormat {
                        message: format!(
                            "active chunk offset {offset} is unavailable during compaction; reload the index from its last saved snapshot before retrying"
                        ),
                    })
            })
            .collect::<Result<Vec<_>>>()?;

        let mut new_chunk_offsets = Vec::new();
        let mut new_metadata_filter_index = MetadataFilterIndex::default();
        let mut new_bm25 = self
            .retrieval
            .bm25
            .as_ref()
            .map(|bm25| Bm25Index::new(bm25.config().clone()));
        for (offset, chunk) in new_chunks.iter().enumerate() {
            let chunk_id =
                usize::try_from(chunk.chunk_id).map_err(|_| RetrievalKitError::InvalidFormat {
                    message: format!(
                        "active chunk ID {} does not fit this platform during compaction; compact and save the index on a platform with wider pointer support first",
                        chunk.chunk_id
                    ),
                })?;
            if new_chunk_offsets.len() <= chunk_id {
                new_chunk_offsets.resize(chunk_id + 1, None);
            }
            new_chunk_offsets[chunk_id] = Some(offset);
            new_metadata_filter_index.insert(offset, &chunk.metadata);
            if let Some(bm25) = &mut new_bm25 {
                bm25.add_chunk(chunk.chunk_id, &chunk.text, true);
            }
        }
        let new_active_offsets = (0..new_chunks.len()).collect::<Vec<_>>();

        self.retrieval.encoded_vectors = new_vectors;
        self.corpus.chunks = new_chunks;
        self.corpus.chunk_offsets = new_chunk_offsets;
        self.corpus.active_offsets = new_active_offsets;
        self.retrieval.metadata_filter_index = new_metadata_filter_index;
        self.retrieval.bm25 = new_bm25;
        self.corpus.generation = self.corpus.generation.next();

        let chunks_after = self.corpus.chunks.len();
        let estimated_bytes_after = self.size_estimate().total_bytes();
        Ok(CompactionReport {
            chunks_before,
            chunks_after,
            chunks_removed: chunks_before.saturating_sub(chunks_after),
            estimated_bytes_before,
            estimated_bytes_after,
            estimated_bytes_reclaimed: estimated_bytes_before.saturating_sub(estimated_bytes_after),
        })
    }

    /// Returns a stored chunk by its internal ID.
    pub fn chunk(&self, chunk_id: ChunkId) -> Option<&StoredChunk> {
        self.corpus.chunk(chunk_id)
    }

    /// Hydrates active chunks in one call while preserving input order.
    ///
    /// Missing, deleted, and superseded IDs produce `None`. Duplicate input IDs
    /// produce duplicate references in the corresponding positions.
    pub fn hydrate_chunks<'a>(&'a self, chunk_ids: &[ChunkId]) -> Vec<Option<&'a StoredChunk>> {
        self.corpus.hydrate_chunks(chunk_ids)
    }

    /// Validates and binds unranked internal IDs to the active corpus generation.
    pub fn candidate_scope(
        &self,
        chunk_ids: impl IntoIterator<Item = ChunkId>,
    ) -> Result<CandidateScope> {
        self.corpus.candidate_scope(chunk_ids)
    }

    /// Resolves stable external identities and binds them to this generation.
    pub fn candidate_scope_for_identities(
        &self,
        identities: impl IntoIterator<Item = ChunkIdentity>,
    ) -> Result<CandidateScope> {
        self.corpus.candidate_scope_for_identities(identities)
    }

    /// Applies a metadata filter using the canonical corpus-owned scope rules.
    pub fn filter_candidate_scope(
        &self,
        scope: &CandidateScope,
        filter: Option<&Filter>,
    ) -> Result<CandidateScope> {
        self.corpus.filter_candidate_scope(scope, filter)
    }

    /// Materializes stable identities using the canonical corpus-owned mapping.
    pub fn candidate_scope_identities(&self, scope: &CandidateScope) -> Result<Vec<ChunkIdentity>> {
        self.corpus.candidate_scope_identities(scope)
    }

    /// Performs exact vector search over active chunks.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.search_vector_candidates(&query.embedding, query.top_k, query.filter.as_ref())
    }

    /// Performs exact vector ranking only inside a validated candidate scope.
    pub fn search_in_candidates(
        &self,
        query: &SearchQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<SearchHit>> {
        self.search_vector_in_candidates(
            &query.embedding,
            query.top_k,
            query.filter.as_ref(),
            scope,
        )
    }

    fn search_vector_in_candidates(
        &self,
        embedding: &[f32],
        top_k: usize,
        filter: Option<&Filter>,
        scope: &CandidateScope,
    ) -> Result<Vec<SearchHit>> {
        self.validate_candidate_scope(scope)?;
        self.validate_dimension(embedding.len())?;
        if top_k == 0 || scope.is_empty() {
            return Ok(Vec::new());
        }

        let encoded_query = self.encode_query_embedding(embedding)?;
        let candidate_offsets = Some(self.scoped_offsets(scope, filter)?);
        if let Some(hits) =
            self.search_i8_offsets(top_k, &encoded_query, filter, &candidate_offsets)?
        {
            return Ok(hits);
        }

        let mut candidates = ScoredCandidateTopK::new(top_k);
        for offset in candidate_offsets.into_iter().flatten() {
            self.score_search_candidate(offset, filter, &encoded_query, &mut candidates)?;
        }
        Ok(self.materialize_search_hits(&candidates.into_sorted_vec()))
    }

    fn search_vector_candidates(
        &self,
        embedding: &[f32],
        top_k: usize,
        filter: Option<&Filter>,
    ) -> Result<Vec<SearchHit>> {
        self.validate_dimension(embedding.len())?;

        if top_k == 0 {
            return Ok(Vec::new());
        }
        let encoded_query = self.encode_query_embedding(embedding)?;

        let candidate_offsets = filter
            .map(|filter| {
                self.retrieval
                    .metadata_filter_index
                    .candidate_offsets(filter)
            })
            .transpose()?
            .flatten();

        if let Some(hits) =
            self.search_i8_offsets(top_k, &encoded_query, filter, &candidate_offsets)?
        {
            return Ok(hits);
        }

        let mut candidates = ScoredCandidateTopK::new(top_k);
        match candidate_offsets {
            Some(offsets) => {
                for offset in offsets {
                    self.score_search_candidate(offset, filter, &encoded_query, &mut candidates)?;
                }
            }
            None => {
                for offset in self.corpus.active_offsets.iter().copied() {
                    self.score_search_candidate(offset, filter, &encoded_query, &mut candidates)?;
                }
            }
        }

        let candidates = candidates.into_sorted_vec();

        Ok(self.materialize_search_hits(&candidates))
    }

    fn search_i8_offsets(
        &self,
        top_k: usize,
        encoded_query: &scoring::EncodedQuery,
        filter: Option<&Filter>,
        candidate_offsets: &Option<Vec<usize>>,
    ) -> Result<Option<Vec<SearchHit>>> {
        let Some((values, scales)) = self.retrieval.encoded_vectors.i8_scalar_quantized_parts()
        else {
            return Ok(None);
        };
        let Some((query_values, query_scale)) = encoded_query.i8_scalar_quantized_parts() else {
            return Ok(None);
        };
        let i8_parts = I8ScoringParts {
            query_values,
            query_scale,
            values,
            scales,
        };

        let mut candidates = ScoredCandidateTopK::new(top_k);
        let offsets = candidate_offsets
            .as_deref()
            .unwrap_or(self.corpus.active_offsets.as_slice());

        match filter {
            Some(filter) => {
                for offset in offsets.iter().copied() {
                    let Some(chunk) = self.corpus.chunks.get(offset) else {
                        continue;
                    };

                    if chunk.deleted || !filter.matches(&chunk.metadata)? {
                        continue;
                    }

                    self.push_i8_candidate(&mut candidates, offset, chunk.chunk_id, i8_parts);
                }
            }
            None => {
                for offset in offsets.iter().copied() {
                    let Some(chunk) = self.corpus.chunks.get(offset) else {
                        continue;
                    };
                    debug_assert!(!chunk.deleted);
                    self.push_i8_candidate(&mut candidates, offset, chunk.chunk_id, i8_parts);
                }
            }
        }

        let candidates = candidates.into_sorted_vec();

        Ok(Some(self.materialize_search_hits(&candidates)))
    }

    fn push_i8_candidate(
        &self,
        candidates: &mut ScoredCandidateTopK,
        offset: usize,
        chunk_id: ChunkId,
        i8_parts: I8ScoringParts<'_>,
    ) {
        let Some(start) = offset.checked_mul(self.retrieval.dimension) else {
            return;
        };
        let Some(end) = start.checked_add(self.retrieval.dimension) else {
            return;
        };
        let Some(chunk_values) = i8_parts.values.get(start..end) else {
            return;
        };
        let Some(&chunk_scale) = i8_parts.scales.get(offset) else {
            return;
        };
        let score = scoring::dot_product_i8(i8_parts.query_values, chunk_values)
            * i8_parts.query_scale
            * chunk_scale;

        candidates.push(ScoredCandidate {
            chunk_id,
            offset,
            score,
        });
    }

    /// Performs BM25 keyword search over active chunks.
    pub fn keyword_search(&self, query: &KeywordQuery) -> Result<Vec<KeywordHit>> {
        self.keyword_search_candidates(&query.text, query.top_k, query.filter.as_ref())
    }

    /// Performs BM25 ranking only inside a validated candidate scope.
    pub fn keyword_search_in_candidates(
        &self,
        query: &KeywordQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<KeywordHit>> {
        self.keyword_search_text_in_candidates(
            &query.text,
            query.top_k,
            query.filter.as_ref(),
            scope,
        )
    }

    fn keyword_search_text_in_candidates(
        &self,
        text: &str,
        top_k: usize,
        filter: Option<&Filter>,
        scope: &CandidateScope,
    ) -> Result<Vec<KeywordHit>> {
        self.validate_candidate_scope(scope)?;
        if top_k == 0 || scope.is_empty() {
            return Ok(Vec::new());
        }

        let effective_scope = self.corpus.filter_candidate_scope(scope, filter)?;
        let bm25_hits =
            self.retrieval
                .require_bm25()?
                .search_top_k_in_scope(text, top_k, &effective_scope);
        Ok(bm25_hits
            .into_iter()
            .filter_map(|hit| {
                let chunk = self.chunk(hit.chunk_id)?;
                if chunk.deleted {
                    return None;
                }
                Some(KeywordHit {
                    chunk_id: chunk.chunk_id,
                    document_id: chunk.document_id.clone(),
                    score: hit.score,
                    matched_terms: hit.matched_terms,
                })
            })
            .collect())
    }

    fn keyword_search_candidates(
        &self,
        text: &str,
        top_k: usize,
        filter: Option<&Filter>,
    ) -> Result<Vec<KeywordHit>> {
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let candidate_offsets = filter
            .map(|filter| {
                self.retrieval
                    .metadata_filter_index
                    .candidate_offsets(filter)
            })
            .transpose()?
            .flatten();
        let allowed_chunk_ids = candidate_offsets
            .as_deref()
            .map(|offsets| self.active_chunk_ids_for_offsets(offsets));

        if allowed_chunk_ids
            .as_ref()
            .is_some_and(|allowed_chunk_ids| allowed_chunk_ids.is_empty())
        {
            return Ok(Vec::new());
        }

        let bm25 = self.retrieval.require_bm25()?;
        let bm25_hits = match (filter, allowed_chunk_ids.as_ref()) {
            (None, _) => bm25.search_top_k(text, top_k),
            (Some(_), Some(allowed_chunk_ids)) => {
                bm25.search_top_k_in_chunks(text, top_k, allowed_chunk_ids)
            }
            (Some(_), None) => bm25.search_all(text),
        };

        let mut hits = Vec::new();
        for keyword_hit in bm25_hits {
            let Some(chunk) = self.chunk(keyword_hit.chunk_id) else {
                continue;
            };

            if chunk.deleted {
                continue;
            }

            if !matches_filter(filter, chunk)? {
                continue;
            }

            hits.push(KeywordHit {
                chunk_id: chunk.chunk_id,
                document_id: chunk.document_id.clone(),
                score: keyword_hit.score,
                matched_terms: keyword_hit.matched_terms,
            });

            if hits.len() == top_k {
                break;
            }
        }

        Ok(hits)
    }

    fn active_chunk_ids_for_offsets(&self, offsets: &[usize]) -> HashSet<ChunkId> {
        let mut chunk_ids = HashSet::with_capacity(offsets.len());
        for offset in offsets {
            let Some(chunk) = self.corpus.chunks.get(*offset) else {
                continue;
            };
            if !chunk.deleted {
                chunk_ids.insert(chunk.chunk_id);
            }
        }
        chunk_ids
    }

    /// Performs hybrid exact vector + BM25 search using the configured fusion strategy.
    pub fn hybrid_search(&self, query: &HybridQuery) -> Result<Vec<HybridHit>> {
        if fusion_uses_vector(query.fusion) {
            self.validate_dimension(query.embedding.len())?;
        }
        if query.top_k == 0 {
            return Ok(Vec::new());
        }

        validate_hybrid_fusion(query.fusion)?;

        let vector_hits = if fusion_uses_vector(query.fusion) {
            self.search_vector_candidates(
                &query.embedding,
                query.vector_top_k,
                query.filter.as_ref(),
            )?
        } else {
            Vec::new()
        };
        let keyword_hits = if fusion_uses_keyword(query.fusion) {
            self.keyword_search_candidates(&query.text, query.keyword_top_k, query.filter.as_ref())?
        } else {
            Vec::new()
        };

        let mut candidates = BTreeMap::<ChunkId, HybridCandidate>::new();
        for (rank_index, hit) in vector_hits.iter().enumerate() {
            let rank = rank_index + 1;
            let candidate = candidates
                .entry(hit.chunk_id)
                .or_insert_with(|| HybridCandidate::new(hit.chunk_id, hit.document_id.clone()));
            candidate.document_id.clone_from(&hit.document_id);
            candidate.vector_score = Some(hit.score);
            candidate.vector_rank = Some(rank);
        }

        for (rank_index, hit) in keyword_hits.iter().enumerate() {
            let rank = rank_index + 1;
            let candidate = candidates
                .entry(hit.chunk_id)
                .or_insert_with(|| HybridCandidate::new(hit.chunk_id, hit.document_id.clone()));
            candidate.document_id.clone_from(&hit.document_id);
            candidate.keyword_score = Some(hit.score);
            candidate.keyword_rank = Some(rank);
            candidate.matched_terms.clone_from(&hit.matched_terms);
        }

        let mut candidates = candidates.into_values().collect::<Vec<_>>();
        score_hybrid_candidates(&mut candidates, query.fusion)?;
        sort_hybrid_candidates(&mut candidates);

        Ok(candidates
            .into_iter()
            .take(query.top_k)
            .filter_map(|candidate| {
                let chunk = self.chunk(candidate.chunk_id)?;
                if chunk.deleted {
                    return None;
                }
                Some(HybridHit {
                    chunk_id: candidate.chunk_id,
                    document_id: candidate.document_id,
                    score: candidate.hybrid_score,
                    vector_score: candidate.vector_score,
                    keyword_score: candidate.keyword_score,
                    trace: HybridTrace {
                        vector_rank: candidate.vector_rank,
                        keyword_rank: candidate.keyword_rank,
                        normalized_vector_score: candidate.normalized_vector_score,
                        normalized_keyword_score: candidate.normalized_keyword_score,
                        matched_terms: candidate.matched_terms,
                        fusion: HybridFusionTrace::from(query.fusion),
                    },
                })
            })
            .collect())
    }

    /// Performs exact vector and BM25 fusion only inside one candidate scope.
    pub fn hybrid_search_in_candidates(
        &self,
        query: &HybridQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<HybridHit>> {
        self.validate_candidate_scope(scope)?;
        if fusion_uses_vector(query.fusion) {
            self.validate_dimension(query.embedding.len())?;
        }
        if query.top_k == 0 || scope.is_empty() {
            return Ok(Vec::new());
        }
        validate_hybrid_fusion(query.fusion)?;

        let vector_hits = if fusion_uses_vector(query.fusion) {
            self.search_vector_in_candidates(
                &query.embedding,
                query.vector_top_k,
                query.filter.as_ref(),
                scope,
            )?
        } else {
            Vec::new()
        };
        let keyword_hits = if fusion_uses_keyword(query.fusion) {
            self.keyword_search_text_in_candidates(
                &query.text,
                query.keyword_top_k,
                query.filter.as_ref(),
                scope,
            )?
        } else {
            Vec::new()
        };

        let mut candidates = BTreeMap::<ChunkId, HybridCandidate>::new();
        for (rank_index, hit) in vector_hits.iter().enumerate() {
            let candidate = candidates
                .entry(hit.chunk_id)
                .or_insert_with(|| HybridCandidate::new(hit.chunk_id, hit.document_id.clone()));
            candidate.vector_score = Some(hit.score);
            candidate.vector_rank = Some(rank_index + 1);
        }
        for (rank_index, hit) in keyword_hits.iter().enumerate() {
            let candidate = candidates
                .entry(hit.chunk_id)
                .or_insert_with(|| HybridCandidate::new(hit.chunk_id, hit.document_id.clone()));
            candidate.keyword_score = Some(hit.score);
            candidate.keyword_rank = Some(rank_index + 1);
            candidate.matched_terms.clone_from(&hit.matched_terms);
        }

        let mut candidates = candidates.into_values().collect::<Vec<_>>();
        score_hybrid_candidates(&mut candidates, query.fusion)?;
        sort_hybrid_candidates(&mut candidates);
        Ok(candidates
            .into_iter()
            .take(query.top_k)
            .filter_map(|candidate| {
                let chunk = self.chunk(candidate.chunk_id)?;
                if chunk.deleted {
                    return None;
                }
                Some(HybridHit {
                    chunk_id: candidate.chunk_id,
                    document_id: candidate.document_id,
                    score: candidate.hybrid_score,
                    vector_score: candidate.vector_score,
                    keyword_score: candidate.keyword_score,
                    trace: HybridTrace {
                        vector_rank: candidate.vector_rank,
                        keyword_rank: candidate.keyword_rank,
                        normalized_vector_score: candidate.normalized_vector_score,
                        normalized_keyword_score: candidate.normalized_keyword_score,
                        matched_terms: candidate.matched_terms,
                        fusion: HybridFusionTrace::from(query.fusion),
                    },
                })
            })
            .collect())
    }

    fn validate_candidate_scope(&self, scope: &CandidateScope) -> Result<()> {
        self.corpus.validate_candidate_scope(scope)
    }

    fn scoped_offsets(
        &self,
        scope: &CandidateScope,
        filter: Option<&Filter>,
    ) -> Result<Vec<usize>> {
        let mut offsets = if scope.is_dense() {
            self.corpus
                .active_offsets
                .iter()
                .copied()
                .filter(|offset| {
                    self.corpus
                        .chunks
                        .get(*offset)
                        .is_some_and(|chunk| scope.contains(chunk.chunk_id))
                })
                .collect::<Vec<_>>()
        } else {
            scope
                .ids()
                .filter_map(|chunk_id| {
                    self.corpus
                        .chunk_offsets
                        .get(usize::try_from(chunk_id).ok()?)?
                        .as_ref()
                        .copied()
                })
                .collect::<Vec<_>>()
        };
        offsets.sort_unstable();

        if let Some(filter) = filter {
            if let Some(mut filter_offsets) = self
                .retrieval
                .metadata_filter_index
                .candidate_offsets(filter)?
            {
                filter_offsets.sort_unstable();
                offsets = intersect_sorted_offsets(&offsets, &filter_offsets);
            }
        }
        Ok(offsets)
    }

    fn validate_dimension(&self, actual: usize) -> Result<()> {
        if actual == self.retrieval.dimension {
            Ok(())
        } else {
            Err(RetrievalKitError::InvalidDimension {
                expected: self.retrieval.dimension,
                actual,
            })
        }
    }

    fn allocate_chunk_id(&mut self) -> ChunkId {
        self.corpus.allocate_chunk_id()
    }

    fn register_chunk_offset(&mut self, chunk_id: ChunkId, offset: usize) {
        self.corpus.register_chunk_offset(chunk_id, offset);
    }

    fn remove_active_offsets(&mut self, offsets: &[usize]) {
        self.corpus.remove_active_offsets(offsets);
    }

    fn remove_chunk_identities_for_record(&mut self, record_id: &RecordId) {
        self.corpus.remove_chunk_identities_for_record(record_id);
    }

    fn push_embedding(&mut self, embedding: &[f32]) {
        match self.retrieval.metric {
            VectorMetric::DotProduct => self.retrieval.encoded_vectors.push(embedding),
            VectorMetric::Cosine => {
                let mut normalized = embedding.to_vec();
                scoring::normalize(&mut normalized);
                self.retrieval.encoded_vectors.push(&normalized);
            }
        }
    }

    fn encode_query_embedding(&self, embedding: &[f32]) -> Result<scoring::EncodedQuery> {
        match self.retrieval.metric {
            VectorMetric::DotProduct => {
                scoring::encode_query(self.retrieval.vector_encoding, embedding)
            }
            VectorMetric::Cosine => {
                let mut normalized = embedding.to_vec();
                scoring::normalize(&mut normalized);
                scoring::encode_query_owned(self.retrieval.vector_encoding, normalized)
            }
        }
    }

    fn rebuild_derived_state_from_loaded_chunks(&mut self) {
        self.corpus.rebuild_offsets_and_versions();
        self.retrieval.metadata_filter_index = MetadataFilterIndex::default();
        for offset in self.corpus.active_offsets.iter().copied() {
            let chunk = &self.corpus.chunks[offset];
            self.retrieval
                .metadata_filter_index
                .insert(offset, &chunk.metadata);
        }
    }

    fn rebuild_bm25_from_loaded_chunks(&mut self) {
        let Some(bm25) = &mut self.retrieval.bm25 else {
            return;
        };
        *bm25 = Bm25Index::new(Bm25Config::default());
        for chunk in &self.corpus.chunks {
            bm25.add_chunk(chunk.chunk_id, &chunk.text, !chunk.deleted);
        }
    }

    fn validate_record_state(&self) -> Result<()> {
        self.corpus.validate()
    }

    fn score_search_candidate(
        &self,
        offset: usize,
        filter: Option<&Filter>,
        encoded_query: &scoring::EncodedQuery,
        hits: &mut ScoredCandidateTopK,
    ) -> Result<()> {
        let Some(chunk) = self.corpus.chunks.get(offset) else {
            return Ok(());
        };

        if chunk.deleted {
            return Ok(());
        }

        if !matches_filter(filter, chunk)? {
            return Ok(());
        }

        let Some(score) = self.retrieval.encoded_vectors.score_at(
            self.retrieval.metric,
            encoded_query,
            offset,
            self.retrieval.dimension,
        ) else {
            return Ok(());
        };

        hits.push(ScoredCandidate {
            chunk_id: chunk.chunk_id,
            offset,
            score,
        });

        Ok(())
    }

    fn materialize_search_hits(&self, candidates: &[ScoredCandidate]) -> Vec<SearchHit> {
        candidates
            .iter()
            .filter_map(|candidate| {
                let chunk = self.corpus.chunks.get(candidate.offset)?;
                Some(SearchHit {
                    chunk_id: candidate.chunk_id,
                    document_id: chunk.document_id.clone(),
                    score: candidate.score,
                    trace: SearchTrace {
                        vector_score: candidate.score,
                    },
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedManifest {
    format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snapshot_id: Option<String>,
    created_with: String,
    #[serde(default = "default_corpus_id")]
    corpus_id: CorpusId,
    #[serde(default)]
    generation: GenerationId,
    dimension: usize,
    metric: VectorMetric,
    vector_count: usize,
    active_chunk_count: usize,
    #[serde(default)]
    retrieval_mode: crate::retrieval_index::RetrievalMode,
    has_bm25: bool,
    #[serde(default)]
    has_records: bool,
    vector_encoding: VectorEncoding,
    vector_bytes: usize,
    chunk_bytes: u64,
    #[serde(default)]
    chunk_uncompressed_bytes: u64,
    #[serde(default)]
    chunk_compression: FileCompression,
    #[serde(default)]
    records_bytes: u64,
    #[serde(default)]
    records_uncompressed_bytes: u64,
    #[serde(default)]
    records_compression: FileCompression,
    bm25_bytes: u64,
    #[serde(default)]
    bm25_uncompressed_bytes: u64,
    #[serde(default)]
    bm25_compression: FileCompression,
    tombstone_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checksums: Option<PersistedChecksums>,
    normalization: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedChecksums {
    algorithm: ChecksumAlgorithm,
    vectors: String,
    chunks: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    records: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bm25: Option<String>,
    tombstones: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedRecordState {
    record_store: RecordStore,
    chunk_identities: Vec<(ChunkIdentity, ChunkId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ChecksumAlgorithm {
    Sha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum FileCompression {
    #[default]
    None,
    Zstd,
}

impl PersistedManifest {
    fn validate(&self) -> Result<()> {
        if !matches!(
            self.format_version,
            LEGACY_FORMAT_VERSION
                | TRANSACTIONAL_FORMAT_VERSION
                | CHECKSUM_FORMAT_VERSION
                | FORMAT_VERSION
        ) {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!("unsupported format version {}", self.format_version),
            });
        }

        match (self.format_version, &self.snapshot_id) {
            (LEGACY_FORMAT_VERSION, None) => {}
            (
                TRANSACTIONAL_FORMAT_VERSION | CHECKSUM_FORMAT_VERSION | FORMAT_VERSION,
                Some(snapshot_id),
            ) if valid_snapshot_id(snapshot_id) => {}
            (LEGACY_FORMAT_VERSION, Some(_)) => {
                return Err(RetrievalKitError::InvalidFormat {
                    message: "legacy format must not reference a snapshot generation".to_owned(),
                });
            }
            (TRANSACTIONAL_FORMAT_VERSION | CHECKSUM_FORMAT_VERSION | FORMAT_VERSION, _) => {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!(
                        "format version {} requires a safe snapshot_id",
                        self.format_version
                    ),
                });
            }
            _ => unreachable!("format version was checked above"),
        }

        if self.created_with != CREATED_WITH {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!("unsupported index creator '{}'", self.created_with),
            });
        }

        if !self.has_bm25 && self.bm25_bytes != 0 {
            return Err(RetrievalKitError::InvalidFormat {
                message: "manifest cannot report bm25 bytes when has_bm25 is false".to_owned(),
            });
        }

        if !self.has_bm25 && self.bm25_uncompressed_bytes != 0 {
            return Err(RetrievalKitError::InvalidFormat {
                message: "manifest cannot report bm25 uncompressed bytes when has_bm25 is false"
                    .to_owned(),
            });
        }

        if !self.has_bm25 && self.bm25_compression != FileCompression::None {
            return Err(RetrievalKitError::InvalidFormat {
                message: "manifest cannot report bm25 compression when has_bm25 is false"
                    .to_owned(),
            });
        }

        if !self.has_records
            && (self.records_bytes != 0
                || self.records_uncompressed_bytes != 0
                || self.records_compression != FileCompression::None)
        {
            return Err(RetrievalKitError::InvalidFormat {
                message: "manifest cannot report record payload data when has_records is false"
                    .to_owned(),
            });
        }

        match (self.format_version, &self.checksums) {
            (CHECKSUM_FORMAT_VERSION | FORMAT_VERSION, Some(checksums)) => {
                checksums.validate(self.has_bm25, self.has_records)?
            }
            (CHECKSUM_FORMAT_VERSION | FORMAT_VERSION, None) => {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!("format version {} requires checksums", self.format_version),
                });
            }
            (LEGACY_FORMAT_VERSION | TRANSACTIONAL_FORMAT_VERSION, None) => {}
            (_, Some(_)) => {
                return Err(RetrievalKitError::InvalidFormat {
                    message: "checksums require format version 3 or newer".to_owned(),
                });
            }
            _ => unreachable!("format version was checked above"),
        }

        Ok(())
    }

    fn data_directory(&self, index_directory: &Path) -> Result<PathBuf> {
        match &self.snapshot_id {
            Some(snapshot_id) if valid_snapshot_id(snapshot_id) => {
                Ok(index_directory.join(SNAPSHOTS_DIRECTORY).join(snapshot_id))
            }
            Some(_) => Err(RetrievalKitError::InvalidFormat {
                message: "snapshot_id contains unsafe path characters".to_owned(),
            }),
            None => Ok(index_directory.to_path_buf()),
        }
    }
}

impl PersistedChecksums {
    fn validate(&self, has_bm25: bool, has_records: bool) -> Result<()> {
        for (name, checksum) in [
            (VECTORS_FILE, Some(self.vectors.as_str())),
            (CHUNKS_FILE, Some(self.chunks.as_str())),
            (RECORDS_FILE, self.records.as_deref()),
            (BM25_FILE, self.bm25.as_deref()),
            (TOMBSTONES_FILE, Some(self.tombstones.as_str())),
        ] {
            match checksum {
                Some(value) if valid_sha256(value) => {}
                Some(_) => {
                    return Err(RetrievalKitError::InvalidFormat {
                        message: format!(
                            "manifest checksum for {name} must be 64 lowercase hex characters"
                        ),
                    });
                }
                None if name == BM25_FILE && !has_bm25 => {}
                None if name == RECORDS_FILE && !has_records => {}
                None => {
                    return Err(RetrievalKitError::InvalidFormat {
                        message: format!("manifest is missing checksum for {name}"),
                    });
                }
            }
        }
        if !has_bm25 && self.bm25.is_some() {
            return Err(RetrievalKitError::InvalidFormat {
                message: "manifest cannot include a bm25 checksum when has_bm25 is false"
                    .to_owned(),
            });
        }
        if !has_records && self.records.is_some() {
            return Err(RetrievalKitError::InvalidFormat {
                message: "manifest cannot include a records checksum when has_records is false"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_snapshot_id(snapshot_id: &str) -> bool {
    !snapshot_id.is_empty()
        && snapshot_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn next_snapshot_id() -> Result<String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RetrievalKitError::Persistence {
            operation: "create snapshot identifier because the system clock is before Unix epoch"
                .to_owned(),
            path: SNAPSHOTS_DIRECTORY.to_owned(),
            cause: "system clock is earlier than 1970-01-01".to_owned(),
        })?
        .as_nanos();
    let sequence = SNAPSHOT_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
    Ok(format!("{nanos}-{}-{sequence}", std::process::id()))
}

fn validate_snapshot_file_sizes(
    manifest: &PersistedManifest,
    snapshot_directory: &Path,
) -> Result<()> {
    let vectors_path = snapshot_directory.join(VECTORS_FILE);
    let vector_bytes = file_size(&vectors_path)?;
    if vector_bytes != manifest.vector_bytes as u64 {
        return Err(RetrievalKitError::InvalidFormat {
            message: format!(
                "manifest vector bytes {} do not match '{}' size {vector_bytes}",
                manifest.vector_bytes,
                vectors_path.display()
            ),
        });
    }
    validate_file_size(&snapshot_directory.join(CHUNKS_FILE), manifest.chunk_bytes)?;
    if manifest.has_records {
        validate_file_size(
            &snapshot_directory.join(RECORDS_FILE),
            manifest.records_bytes,
        )?;
    }
    validate_file_size(
        &snapshot_directory.join(TOMBSTONES_FILE),
        manifest.tombstone_bytes,
    )?;
    if manifest.has_bm25 {
        validate_file_size(&snapshot_directory.join(BM25_FILE), manifest.bm25_bytes)?;
    }
    Ok(())
}

fn validate_snapshot_checksums(
    manifest: &PersistedManifest,
    snapshot_directory: &Path,
) -> Result<()> {
    let Some(checksums) = &manifest.checksums else {
        return Ok(());
    };
    validate_file_checksum(&snapshot_directory.join(VECTORS_FILE), &checksums.vectors)?;
    validate_file_checksum(&snapshot_directory.join(CHUNKS_FILE), &checksums.chunks)?;
    if let Some(expected) = &checksums.records {
        validate_file_checksum(&snapshot_directory.join(RECORDS_FILE), expected)?;
    }
    validate_file_checksum(
        &snapshot_directory.join(TOMBSTONES_FILE),
        &checksums.tombstones,
    )?;
    if let Some(expected) = &checksums.bm25 {
        validate_file_checksum(&snapshot_directory.join(BM25_FILE), expected)?;
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = fs::File::open(path)
        .map_err(|error| persistence_error("open for checksum", path, &error))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|error| persistence_error("read for checksum", path, &error))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_file_checksum(path: &Path, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(RetrievalKitError::CorruptIndex {
            path: path.display().to_string(),
            message: format!("SHA-256 checksum mismatch: expected {expected}, found {actual}"),
        })
    }
}

fn cleanup_unreferenced_snapshots(snapshots_directory: &Path, active_snapshot_id: &str) {
    let Ok(entries) = fs::read_dir(snapshots_directory) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() != active_snapshot_id {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn cleanup_legacy_files(directory: &Path) {
    for file_name in [
        VECTORS_FILE,
        CHUNKS_FILE,
        RECORDS_FILE,
        BM25_FILE,
        TOMBSTONES_FILE,
    ] {
        let _ = fs::remove_file(directory.join(file_name));
    }
}

fn cleanup_temporary_manifests(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with("manifest.") && file_name.ends_with(".tmp") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| RetrievalKitError::InvalidFormat {
            message: format!("could not serialize '{}': {error}", path.display()),
        })?;
    write_file(path, &bytes)
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = read_file(path)?;
    serde_json::from_slice(&bytes).map_err(|error| RetrievalKitError::InvalidFormat {
        message: format!("could not parse '{}': {error}", path.display()),
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| persistence_error("open for writing", path, &error))?;
    file.write_all(bytes)
        .map_err(|error| persistence_error("write", path, &error))?;
    file.sync_all()
        .map_err(|error| persistence_error("sync", path, &error))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| persistence_error("sync directory", path, &error))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| persistence_error("read", path, &error))
}

fn compress_payload(path: &Path, bytes: &[u8], compression: FileCompression) -> Result<Vec<u8>> {
    match compression {
        FileCompression::None => Ok(bytes.to_vec()),
        FileCompression::Zstd => {
            zstd::stream::encode_all(Cursor::new(bytes), ZSTD_COMPRESSION_LEVEL).map_err(|error| {
                RetrievalKitError::InvalidFormat {
                    message: format!("could not zstd-compress '{}': {error}", path.display()),
                }
            })
        }
    }
}

fn read_payload_file(
    path: &Path,
    compression: FileCompression,
    expected_uncompressed_bytes: u64,
) -> Result<Vec<u8>> {
    let bytes = read_file(path)?;
    let payload = match compression {
        FileCompression::None => bytes,
        FileCompression::Zstd => zstd::stream::decode_all(Cursor::new(bytes)).map_err(|error| {
            RetrievalKitError::InvalidFormat {
                message: format!("could not zstd-decompress '{}': {error}", path.display()),
            }
        })?,
    };

    validate_uncompressed_size(path, expected_uncompressed_bytes, payload.len())?;
    Ok(payload)
}

fn file_size(path: &Path) -> Result<u64> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| persistence_error("stat", path, &error))
}

fn file_size_if_exists(path: &Path) -> Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(persistence_error("stat", path, &error)),
    }
}

fn validate_file_size(path: &Path, expected: u64) -> Result<()> {
    let actual = file_size(path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(RetrievalKitError::InvalidFormat {
            message: format!(
                "manifest file size {expected} does not match '{}' size {actual}",
                path.display()
            ),
        })
    }
}

fn validate_uncompressed_size(path: &Path, expected: u64, actual: usize) -> Result<()> {
    if expected == 0 {
        return Ok(());
    }

    let actual = checked_usize_to_u64(actual, "uncompressed file size")?;
    if actual == expected {
        Ok(())
    } else {
        Err(RetrievalKitError::InvalidFormat {
            message: format!(
                "manifest uncompressed file size {expected} does not match '{}' size {actual}",
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
            return Err(RetrievalKitError::InvalidFormat {
                message: format!("bm25 active chunk {chunk_id} is missing from chunk records"),
            });
        };

        if chunk.deleted {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!("bm25 active chunk {chunk_id} is tombstoned"),
            });
        }
    }

    for chunk_id in bm25.chunk_length_ids() {
        if !chunk_by_id.contains_key(chunk_id) {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!("bm25 length chunk {chunk_id} is missing from chunk records"),
            });
        }
    }

    Ok(())
}

fn encode_chunks(chunks: &[StoredChunk]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let metadata_fields = collect_metadata_fields(chunks)?;
    bytes.extend_from_slice(CHUNKS_MAGIC);
    write_u32(&mut bytes, CHUNKS_FORMAT_VERSION);
    write_u64(
        &mut bytes,
        checked_usize_to_u64(chunks.len(), "chunk count")?,
    );
    write_u32(
        &mut bytes,
        checked_usize_to_u32(metadata_fields.len(), "metadata dictionary field count")?,
    );
    for field in &metadata_fields {
        write_string(&mut bytes, field)?;
    }

    for chunk in chunks {
        write_u64(&mut bytes, chunk.chunk_id);
        write_u64(&mut bytes, chunk.version);
        write_bool(&mut bytes, chunk.deleted);
        write_string(&mut bytes, &chunk.document_id)?;
        write_string(&mut bytes, &chunk.text)?;
        write_metadata_v2(&mut bytes, &chunk.metadata, &metadata_fields)?;
    }

    Ok(bytes)
}

fn decode_chunks(bytes: &[u8]) -> Result<Vec<StoredChunk>> {
    let mut reader = ByteReader::new(bytes);
    if reader.read_exact(CHUNKS_MAGIC.len())? != CHUNKS_MAGIC {
        return Err(RetrievalKitError::InvalidFormat {
            message: "chunk file has invalid magic".to_owned(),
        });
    }

    let format_version = reader.read_u32()?;
    let chunk_count = checked_u64_to_usize(reader.read_u64()?, "chunk count")?;
    let metadata_fields = match format_version {
        LEGACY_CHUNKS_FORMAT_VERSION => Vec::new(),
        CHUNKS_FORMAT_VERSION => {
            let field_count =
                checked_u32_to_usize(reader.read_u32()?, "metadata dictionary field count")?;
            let mut fields = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                fields.push(reader.read_string()?);
            }
            fields
        }
        _ => {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!("unsupported chunk file version {format_version}"),
            })
        }
    };

    let mut chunks = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        let chunk_id = reader.read_u64()?;
        let version = reader.read_u64()?;
        let deleted = reader.read_bool()?;
        let document_id = reader.read_string()?;
        let text = reader.read_string()?;
        let metadata = if format_version == LEGACY_CHUNKS_FORMAT_VERSION {
            reader.read_metadata_v1()?
        } else {
            reader.read_metadata_v2(&metadata_fields)?
        };
        chunks.push(StoredChunk {
            chunk_id,
            version,
            deleted,
            document_id,
            text,
            metadata,
        });
    }

    reader.finish()?;
    Ok(chunks)
}

fn collect_metadata_fields(chunks: &[StoredChunk]) -> Result<Vec<String>> {
    let mut fields = BTreeMap::<String, u32>::new();
    for chunk in chunks {
        for field in chunk.metadata.keys() {
            if !fields.contains_key(field) {
                let field_id = checked_usize_to_u32(fields.len(), "metadata dictionary field id")?;
                fields.insert(field.clone(), field_id);
            }
        }
    }

    Ok(fields.into_keys().collect())
}

fn write_metadata_v2(
    bytes: &mut Vec<u8>,
    metadata: &Metadata,
    metadata_fields: &[String],
) -> Result<()> {
    write_u32(
        bytes,
        checked_usize_to_u32(metadata.len(), "metadata field count")?,
    );
    for (field, value) in metadata {
        let field_id =
            metadata_fields
                .binary_search(field)
                .map_err(|_| RetrievalKitError::InvalidFormat {
                    message: format!("metadata field '{field}' is missing from dictionary"),
                })?;
        write_u32(
            bytes,
            checked_usize_to_u32(field_id, "metadata dictionary field id")?,
        );
        match value {
            crate::metadata::MetadataValue::String(value) => {
                write_u8(bytes, METADATA_STRING);
                write_string(bytes, value)?;
            }
            crate::metadata::MetadataValue::Integer(value) => {
                write_u8(bytes, METADATA_INTEGER);
                write_var_i64(bytes, *value);
            }
            crate::metadata::MetadataValue::Float(value) => {
                write_u8(bytes, METADATA_FLOAT);
                write_f64(bytes, *value);
            }
            crate::metadata::MetadataValue::Boolean(value) => {
                write_u8(bytes, METADATA_BOOLEAN);
                write_bool(bytes, *value);
            }
            crate::metadata::MetadataValue::TimestampMillis(value) => {
                write_u8(bytes, METADATA_TIMESTAMP_MILLIS);
                write_var_i64(bytes, *value);
            }
        }
    }
    Ok(())
}

fn write_string(bytes: &mut Vec<u8>, value: &str) -> Result<()> {
    write_u32(bytes, checked_usize_to_u32(value.len(), "string length")?);
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_bool(bytes: &mut Vec<u8>, value: bool) {
    write_u8(bytes, u8::from(value));
}

fn write_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_f64(bytes: &mut Vec<u8>, value: f64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_var_i64(bytes: &mut Vec<u8>, value: i64) {
    write_var_u64(bytes, zigzag_i64(value));
}

fn write_var_u64(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push(((value & 0x7f) as u8) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn zigzag_i64(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn unzigzag_i64(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

struct ByteReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_metadata_v1(&mut self) -> Result<Metadata> {
        let field_count = checked_u32_to_usize(self.read_u32()?, "metadata field count")?;
        let mut metadata = Metadata::new();
        for _ in 0..field_count {
            let field = self.read_string()?;
            let value = match self.read_u8()? {
                METADATA_STRING => crate::metadata::MetadataValue::String(self.read_string()?),
                METADATA_INTEGER => crate::metadata::MetadataValue::Integer(self.read_i64()?),
                METADATA_FLOAT => crate::metadata::MetadataValue::Float(self.read_f64()?),
                METADATA_BOOLEAN => crate::metadata::MetadataValue::Boolean(self.read_bool()?),
                METADATA_TIMESTAMP_MILLIS => {
                    crate::metadata::MetadataValue::TimestampMillis(self.read_i64()?)
                }
                value_type => {
                    return Err(RetrievalKitError::InvalidFormat {
                        message: format!("unknown metadata value type {value_type}"),
                    })
                }
            };
            metadata.insert(field, value);
        }
        Ok(metadata)
    }

    fn read_metadata_v2(&mut self, metadata_fields: &[String]) -> Result<Metadata> {
        let field_count = checked_u32_to_usize(self.read_u32()?, "metadata field count")?;
        let mut metadata = Metadata::new();
        for _ in 0..field_count {
            let field_id = checked_u32_to_usize(self.read_u32()?, "metadata dictionary field id")?;
            let Some(field) = metadata_fields.get(field_id) else {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!("metadata dictionary field id {field_id} is out of bounds"),
                });
            };
            let value = match self.read_u8()? {
                METADATA_STRING => crate::metadata::MetadataValue::String(self.read_string()?),
                METADATA_INTEGER => crate::metadata::MetadataValue::Integer(self.read_var_i64()?),
                METADATA_FLOAT => crate::metadata::MetadataValue::Float(self.read_f64()?),
                METADATA_BOOLEAN => crate::metadata::MetadataValue::Boolean(self.read_bool()?),
                METADATA_TIMESTAMP_MILLIS => {
                    crate::metadata::MetadataValue::TimestampMillis(self.read_var_i64()?)
                }
                value_type => {
                    return Err(RetrievalKitError::InvalidFormat {
                        message: format!("unknown metadata value type {value_type}"),
                    })
                }
            };
            metadata.insert(field.clone(), value);
        }
        Ok(metadata)
    }

    fn read_string(&mut self) -> Result<String> {
        let len = checked_u32_to_usize(self.read_u32()?, "string length")?;
        let bytes = self.read_exact(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|error| RetrievalKitError::InvalidFormat {
            message: format!("invalid UTF-8 string in chunk file: {error}"),
        })
    }

    fn read_bool(&mut self) -> Result<bool> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(RetrievalKitError::InvalidFormat {
                message: format!("invalid boolean byte {value}"),
            }),
        }
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.read_exact(std::mem::size_of::<u32>())?
                .try_into()
                .expect("u32 chunk size"),
        ))
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.read_exact(std::mem::size_of::<u64>())?
                .try_into()
                .expect("u64 chunk size"),
        ))
    }

    fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(
            self.read_exact(std::mem::size_of::<i64>())?
                .try_into()
                .expect("i64 chunk size"),
        ))
    }

    fn read_f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(
            self.read_exact(std::mem::size_of::<f64>())?
                .try_into()
                .expect("f64 chunk size"),
        ))
    }

    fn read_var_i64(&mut self) -> Result<i64> {
        Ok(unzigzag_i64(self.read_var_u64()?))
    }

    fn read_var_u64(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.read_u8()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }

        Err(RetrievalKitError::InvalidFormat {
            message: "variable-length integer is too long".to_owned(),
        })
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| RetrievalKitError::InvalidFormat {
                message: "chunk reader offset overflow".to_owned(),
            })?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(RetrievalKitError::InvalidFormat {
                message: "chunk file ended unexpectedly".to_owned(),
            });
        };
        self.offset = end;
        Ok(bytes)
    }

    fn finish(&self) -> Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(RetrievalKitError::InvalidFormat {
                message: format!(
                    "chunk file has {} trailing bytes",
                    self.bytes.len() - self.offset
                ),
            })
        }
    }
}

fn checked_usize_to_u32(value: usize, label: &str) -> Result<u32> {
    value
        .try_into()
        .map_err(|_| RetrievalKitError::InvalidFormat {
            message: format!("{label} does not fit in u32"),
        })
}

fn checked_usize_to_u64(value: usize, label: &str) -> Result<u64> {
    value
        .try_into()
        .map_err(|_| RetrievalKitError::InvalidFormat {
            message: format!("{label} does not fit in u64"),
        })
}

fn checked_u32_to_usize(value: u32, label: &str) -> Result<usize> {
    value
        .try_into()
        .map_err(|_| RetrievalKitError::InvalidFormat {
            message: format!("{label} does not fit in usize"),
        })
}

fn checked_u64_to_usize(value: u64, label: &str) -> Result<usize> {
    value
        .try_into()
        .map_err(|_| RetrievalKitError::InvalidFormat {
            message: format!("{label} does not fit in usize"),
        })
}

fn persistence_error(operation: &str, path: &Path, error: &std::io::Error) -> RetrievalKitError {
    RetrievalKitError::Persistence {
        operation: operation.to_owned(),
        path: display_path(path),
        cause: error.to_string(),
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

fn intersect_sorted_offsets(left: &[usize], right: &[usize]) -> Vec<usize> {
    let mut intersection = Vec::with_capacity(left.len().min(right.len()));
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            Ordering::Less => left_index += 1,
            Ordering::Greater => right_index += 1,
            Ordering::Equal => {
                intersection.push(left[left_index]);
                left_index += 1;
                right_index += 1;
            }
        }
    }
    intersection
}

fn merge_metadata(document_metadata: &Metadata, chunk_metadata: Metadata) -> Metadata {
    let mut metadata = document_metadata.clone();
    metadata.extend(chunk_metadata);
    metadata
}

fn record_value_from_metadata(value: &MetadataValue) -> RecordValue {
    match value {
        MetadataValue::String(value) => RecordValue::String(value.clone()),
        MetadataValue::Integer(value) | MetadataValue::TimestampMillis(value) => {
            RecordValue::I64(*value)
        }
        MetadataValue::Float(value) => RecordValue::F64(*value),
        MetadataValue::Boolean(value) => RecordValue::Bool(*value),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScoredCandidate {
    chunk_id: ChunkId,
    offset: usize,
    score: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct HybridCandidate {
    chunk_id: ChunkId,
    document_id: String,
    vector_score: Option<f32>,
    vector_rank: Option<usize>,
    keyword_score: Option<f32>,
    keyword_rank: Option<usize>,
    normalized_vector_score: Option<f32>,
    normalized_keyword_score: Option<f32>,
    matched_terms: Vec<String>,
    hybrid_score: f32,
}

impl HybridCandidate {
    fn new(chunk_id: ChunkId, document_id: String) -> Self {
        Self {
            chunk_id,
            document_id,
            vector_score: None,
            vector_rank: None,
            keyword_score: None,
            keyword_rank: None,
            normalized_vector_score: None,
            normalized_keyword_score: None,
            matched_terms: Vec::new(),
            hybrid_score: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
struct I8ScoringParts<'a> {
    query_values: &'a [i8],
    query_scale: f32,
    values: &'a [i8],
    scales: &'a [f32],
}

struct ScoredCandidateTopK {
    top_k: usize,
    heap: BinaryHeap<HeapScoredCandidate>,
}

impl ScoredCandidateTopK {
    fn new(top_k: usize) -> Self {
        Self {
            top_k,
            heap: BinaryHeap::with_capacity(top_k),
        }
    }

    fn push(&mut self, candidate: ScoredCandidate) {
        if self.heap.len() < self.top_k {
            self.heap.push(HeapScoredCandidate(candidate));
            return;
        }

        let Some(worst) = self.heap.peek() else {
            return;
        };

        if hit_ranks_before(&candidate, &worst.0) {
            self.heap.pop();
            self.heap.push(HeapScoredCandidate(candidate));
        }
    }

    fn into_sorted_vec(self) -> Vec<ScoredCandidate> {
        let mut hits = self
            .heap
            .into_iter()
            .map(|candidate| candidate.0)
            .collect::<Vec<_>>();
        sort_scored_candidates(&mut hits);
        hits
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HeapScoredCandidate(ScoredCandidate);

impl Eq for HeapScoredCandidate {}

impl Ord for HeapScoredCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .score
            .total_cmp(&self.0.score)
            .then_with(|| self.0.chunk_id.cmp(&other.0.chunk_id))
    }
}

impl PartialOrd for HeapScoredCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn sort_scored_candidates(hits: &mut [ScoredCandidate]) {
    hits.sort_by(compare_hits);
}

fn compare_hits(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn hit_ranks_before(left: &ScoredCandidate, right: &ScoredCandidate) -> bool {
    compare_hits(left, right).is_lt()
}

fn validate_hybrid_fusion(fusion: HybridFusion) -> Result<()> {
    match fusion {
        HybridFusion::ReciprocalRank { rrf_k } => validate_positive_finite(rrf_k, "rrf_k"),
        HybridFusion::WeightedNormalizedScore {
            vector_weight,
            keyword_weight,
        } => {
            if !vector_weight.is_finite() || vector_weight < 0.0 {
                return Err(RetrievalKitError::InvalidFormat {
                    message: "vector_weight must be finite and non-negative".to_owned(),
                });
            }
            if !keyword_weight.is_finite() || keyword_weight < 0.0 {
                return Err(RetrievalKitError::InvalidFormat {
                    message: "keyword_weight must be finite and non-negative".to_owned(),
                });
            }
            if vector_weight == 0.0 && keyword_weight == 0.0 {
                return Err(RetrievalKitError::InvalidFormat {
                    message: "at least one hybrid fusion weight must be greater than zero"
                        .to_owned(),
                });
            }
            Ok(())
        }
    }
}

fn fusion_uses_vector(fusion: HybridFusion) -> bool {
    match fusion {
        HybridFusion::ReciprocalRank { .. } => true,
        HybridFusion::WeightedNormalizedScore { vector_weight, .. } => vector_weight > 0.0,
    }
}

fn fusion_uses_keyword(fusion: HybridFusion) -> bool {
    match fusion {
        HybridFusion::ReciprocalRank { .. } => true,
        HybridFusion::WeightedNormalizedScore { keyword_weight, .. } => keyword_weight > 0.0,
    }
}

fn validate_positive_finite(value: f32, label: &str) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(RetrievalKitError::InvalidFormat {
            message: format!("{label} must be finite and greater than zero"),
        })
    }
}

fn rrf_component(rank: Option<usize>, rrf_k: f32) -> f32 {
    rank.map_or(0.0, |rank| 1.0 / (rrf_k + rank as f32))
}

fn score_hybrid_candidates(candidates: &mut [HybridCandidate], fusion: HybridFusion) -> Result<()> {
    validate_hybrid_fusion(fusion)?;
    match fusion {
        HybridFusion::ReciprocalRank { rrf_k } => {
            for candidate in candidates {
                candidate.normalized_vector_score = None;
                candidate.normalized_keyword_score = None;
                candidate.hybrid_score = rrf_component(candidate.vector_rank, rrf_k)
                    + rrf_component(candidate.keyword_rank, rrf_k);
            }
        }
        HybridFusion::WeightedNormalizedScore {
            vector_weight,
            keyword_weight,
        } => {
            let vector_range = score_range(candidates.iter().filter_map(|c| c.vector_score));
            let keyword_range = score_range(candidates.iter().filter_map(|c| c.keyword_score));

            for candidate in candidates {
                let normalized_vector_score = normalize_score(candidate.vector_score, vector_range);
                let normalized_keyword_score =
                    normalize_score(candidate.keyword_score, keyword_range);
                candidate.normalized_vector_score = normalized_vector_score;
                candidate.normalized_keyword_score = normalized_keyword_score;
                candidate.hybrid_score = vector_weight * normalized_vector_score.unwrap_or(0.0)
                    + keyword_weight * normalized_keyword_score.unwrap_or(0.0);
            }
        }
    }
    Ok(())
}

fn score_range(scores: impl Iterator<Item = f32>) -> Option<(f32, f32)> {
    let mut min_score = f32::INFINITY;
    let mut max_score = f32::NEG_INFINITY;
    let mut found = false;

    for score in scores {
        if !score.is_finite() {
            continue;
        }
        min_score = min_score.min(score);
        max_score = max_score.max(score);
        found = true;
    }

    found.then_some((min_score, max_score))
}

fn normalize_score(score: Option<f32>, range: Option<(f32, f32)>) -> Option<f32> {
    let score = score?;
    if !score.is_finite() {
        return None;
    }
    let (min_score, max_score) = range?;
    let width = max_score - min_score;
    if width <= f32::EPSILON {
        Some(1.0)
    } else {
        Some((score - min_score) / width)
    }
}

fn sort_hybrid_candidates(candidates: &mut [HybridCandidate]) {
    candidates.sort_by(compare_hybrid_candidates);
}

fn compare_hybrid_candidates(left: &HybridCandidate, right: &HybridCandidate) -> Ordering {
    right
        .hybrid_score
        .total_cmp(&left.hybrid_score)
        .then_with(|| optional_rank_cmp(left.vector_rank, right.vector_rank))
        .then_with(|| optional_rank_cmp(left.keyword_rank, right.keyword_rank))
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn optional_rank_cmp(left: Option<usize>, right: Option<usize>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{Metadata, MetadataValue};

    #[test]
    fn exact_vector_index_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ExactVectorIndex>();
    }

    #[test]
    fn immutable_index_supports_concurrent_read_only_searches() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("parallel local search", vec![1.0, 0.0])],
            )
            .unwrap();
        let index = std::sync::Arc::new(index);
        let start = std::sync::Arc::new(std::sync::Barrier::new(8));

        let workers = (0..8)
            .map(|_| {
                let index = std::sync::Arc::clone(&index);
                let start = std::sync::Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    let exact = index.search(&SearchQuery::new(vec![1.0, 0.0], 1)).unwrap();
                    let keyword = index
                        .keyword_search(&KeywordQuery::new("parallel", 1))
                        .unwrap();
                    let hybrid = index
                        .hybrid_search(&HybridQuery::new("parallel", vec![1.0, 0.0], 1))
                        .unwrap();
                    (exact[0].chunk_id, keyword[0].chunk_id, hybrid[0].chunk_id)
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            assert_eq!(worker.join().unwrap(), (0, 0, 0));
        }
    }

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
        path.push(format!("retrievalkit-{name}-{}", std::process::id()));
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
            RetrievalKitError::InvalidDimension {
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
            RetrievalKitError::InvalidDimension {
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
    fn compaction_removes_tombstones_and_preserves_active_ids_and_results() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        let first_ids = index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("old version", vec![1.0, 0.0])],
            )
            .unwrap();
        let replacement_ids = index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("replacement searchable", vec![0.0, 1.0])],
            )
            .unwrap();
        let deleted_ids = index
            .upsert_document(
                document("doc-2"),
                vec![chunk_input("deleted text", vec![1.0, 0.0])],
            )
            .unwrap();
        assert_eq!(index.delete_document("doc-2"), 1);

        let vector_before = index.search(&SearchQuery::new(vec![0.0, 1.0], 10)).unwrap();
        let keyword_before = index
            .keyword_search(&KeywordQuery::new("replacement", 10))
            .unwrap();
        let report = index.compact().unwrap();

        assert_eq!(report.chunks_before, 3);
        assert_eq!(report.chunks_after, 1);
        assert_eq!(report.chunks_removed, 2);
        assert!(report.estimated_bytes_reclaimed > 0);
        assert_eq!(
            report.estimated_bytes_before - report.estimated_bytes_after,
            report.estimated_bytes_reclaimed
        );
        assert_eq!(index.len(), 1);
        assert_eq!(index.active_chunk_count(), 1);
        assert!(index.chunk(first_ids[0]).is_none());
        assert!(index.chunk(deleted_ids[0]).is_none());
        assert_eq!(
            index.chunk(replacement_ids[0]).unwrap().text,
            "replacement searchable"
        );
        assert_eq!(
            index.search(&SearchQuery::new(vec![0.0, 1.0], 10)).unwrap(),
            vector_before
        );
        assert_eq!(
            index
                .keyword_search(&KeywordQuery::new("replacement", 10))
                .unwrap(),
            keyword_before
        );

        let next_ids = index
            .upsert_document(document("doc-3"), vec![chunk_input("next", vec![1.0, 0.0])])
            .unwrap();
        assert!(next_ids[0] > deleted_ids[0]);
    }

    #[test]
    fn compaction_preserves_every_supported_vector_encoding() {
        for encoding in [
            VectorEncoding::F32,
            VectorEncoding::F16,
            VectorEncoding::BF16,
            VectorEncoding::I8ScalarQuantized,
        ] {
            let mut index = ExactVectorIndex::try_with_config(
                IndexConfig::new(3, VectorMetric::DotProduct).with_vector_encoding(encoding),
            )
            .unwrap();
            index
                .upsert_document(
                    document("deleted"),
                    vec![chunk_input("deleted", vec![0.0, 1.0, 0.0])],
                )
                .unwrap();
            index.delete_document("deleted");
            index
                .upsert_document(
                    document("active"),
                    vec![chunk_input("active", vec![1.0, 0.0, 0.0])],
                )
                .unwrap();
            let before = index
                .search(&SearchQuery::new(vec![1.0, 0.0, 0.0], 1))
                .unwrap();

            index.compact().unwrap();

            assert_eq!(
                index
                    .search(&SearchQuery::new(vec![1.0, 0.0, 0.0], 1))
                    .unwrap(),
                before,
                "encoding {encoding:?}"
            );
        }
    }

    #[test]
    fn compaction_is_idempotent_when_no_tombstones_exist() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("active", vec![1.0, 0.0])],
            )
            .unwrap();

        let first = index.compact().unwrap();
        let second = index.compact().unwrap();

        assert_eq!(first.chunks_removed, 0);
        assert_eq!(second.chunks_removed, 0);
        assert_eq!(second.estimated_bytes_reclaimed, 0);
    }

    #[test]
    fn compaction_failure_does_not_partially_replace_storage() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("deleted"),
                vec![chunk_input("deleted", vec![1.0, 0.0])],
            )
            .unwrap();
        index.delete_document("deleted");
        index
            .upsert_document(
                document("active"),
                vec![chunk_input("active", vec![0.0, 1.0])],
            )
            .unwrap();
        index.corpus.active_offsets.push(999);
        let chunks_before = index.corpus.chunks.clone();
        let vectors_before = index.retrieval.encoded_vectors.to_payload_bytes();
        let active_offsets_before = index.corpus.active_offsets.clone();

        let error = index.compact().unwrap_err();

        assert!(error.to_string().contains("vector row 999 is unavailable"));
        assert_eq!(index.corpus.chunks, chunks_before);
        assert_eq!(
            index.retrieval.encoded_vectors.to_payload_bytes(),
            vectors_before
        );
        assert_eq!(index.corpus.active_offsets, active_offsets_before);
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

        assert_eq!(
            estimate.vector_bytes,
            2 * std::mem::size_of::<i8>() + std::mem::size_of::<f32>()
        );
        assert_eq!(estimate.chunk_record_bytes, std::mem::size_of::<ChunkId>());
        assert_eq!(estimate.document_id_bytes, "doc-1".len());
        assert_eq!(estimate.text_bytes, "chunk 1".len());
        assert_eq!(estimate.metadata_bytes, "source".len() + "notes".len());
        assert_eq!(estimate.tombstone_bytes, std::mem::size_of::<bool>());
        assert_eq!(estimate.version_bytes, std::mem::size_of::<u64>());
        assert_eq!(
            estimate.chunk_offset_bytes,
            2 * std::mem::size_of::<Option<usize>>() + std::mem::size_of::<usize>()
        );
        assert!(estimate.bm25_bytes > 0);
        assert!(estimate.metadata_filter_bytes > 0);
        assert_eq!(
            estimate.total_bytes(),
            estimate.vector_bytes + estimate.auxiliary_bytes()
        );
    }

    #[test]
    fn binary_chunk_records_round_trip_metadata_values() {
        let mut stored_chunk = StoredChunk {
            chunk_id: 42,
            document_id: "doc-42".to_owned(),
            text: "compact chunk text".to_owned(),
            metadata: Metadata::new(),
            deleted: true,
            version: 7,
        };
        stored_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        stored_chunk
            .metadata
            .insert("rank".to_owned(), MetadataValue::Integer(5));
        stored_chunk
            .metadata
            .insert("score".to_owned(), MetadataValue::Float(0.25));
        stored_chunk
            .metadata
            .insert("archived".to_owned(), MetadataValue::Boolean(false));
        stored_chunk.metadata.insert(
            "created".to_owned(),
            MetadataValue::TimestampMillis(1_700_000_000),
        );

        let encoded = encode_chunks(&[stored_chunk.clone()]).unwrap();
        let decoded = decode_chunks(&encoded).unwrap();

        assert_eq!(decoded, vec![stored_chunk]);
    }

    #[test]
    fn binary_chunk_records_dictionary_encode_repeated_metadata_fields() {
        let repeated_field = "__bench_filter_bucket_with_a_long_name";
        let chunks = (0..128)
            .map(|chunk_id| {
                let mut stored_chunk = StoredChunk {
                    chunk_id,
                    document_id: format!("doc-{chunk_id}"),
                    text: format!("chunk {chunk_id}"),
                    metadata: Metadata::new(),
                    deleted: false,
                    version: 1,
                };
                stored_chunk.metadata.insert(
                    repeated_field.to_owned(),
                    MetadataValue::Integer((chunk_id % 10) as i64),
                );
                stored_chunk
            })
            .collect::<Vec<_>>();

        let encoded = encode_chunks(&chunks).unwrap();
        let legacy_size =
            legacy_chunk_encoding_size_for_repeated_integer_field(&chunks, repeated_field);
        let decoded = decode_chunks(&encoded).unwrap();

        assert_eq!(decoded, chunks);
        assert!(encoded.len() < legacy_size);
    }

    #[test]
    fn binary_chunk_records_reject_bad_magic() {
        let error = decode_chunks(b"NOPE").unwrap_err();

        assert!(matches!(error, RetrievalKitError::InvalidFormat { .. }));
    }

    fn legacy_chunk_encoding_size_for_repeated_integer_field(
        chunks: &[StoredChunk],
        field: &str,
    ) -> usize {
        let header_bytes =
            CHUNKS_MAGIC.len() + std::mem::size_of::<u32>() + std::mem::size_of::<u64>();
        header_bytes
            + chunks
                .iter()
                .map(|chunk| {
                    std::mem::size_of::<u64>()
                        + std::mem::size_of::<u64>()
                        + std::mem::size_of::<u8>()
                        + string_encoding_size(&chunk.document_id)
                        + string_encoding_size(&chunk.text)
                        + std::mem::size_of::<u32>()
                        + string_encoding_size(field)
                        + std::mem::size_of::<u8>()
                        + std::mem::size_of::<i64>()
                })
                .sum::<usize>()
    }

    fn string_encoding_size(value: &str) -> usize {
        std::mem::size_of::<u32>() + value.len()
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
        assert!(file_sizes.chunks_bytes < 512);
        assert_eq!(
            ExactVectorIndex::persisted_file_sizes(&directory).unwrap(),
            file_sizes
        );
        let manifest: PersistedManifest = read_json_file(&directory.join(MANIFEST_FILE)).unwrap();
        assert_eq!(manifest.format_version, FORMAT_VERSION);
        assert!(manifest.snapshot_id.is_some());
        assert_eq!(manifest.chunk_compression, FileCompression::Zstd);
        assert!(manifest.chunk_uncompressed_bytes > 0);
        assert_eq!(manifest.bm25_compression, FileCompression::Zstd);
        assert!(manifest.bm25_uncompressed_bytes > 0);
        let checksums = manifest.checksums.as_ref().unwrap();
        assert_eq!(checksums.algorithm, ChecksumAlgorithm::Sha256);
        assert!(valid_sha256(&checksums.vectors));
        assert!(valid_sha256(&checksums.chunks));
        assert!(valid_sha256(checksums.bm25.as_deref().unwrap()));
        assert!(valid_sha256(&checksums.tombstones));

        ExactVectorIndex::validate_dir(&directory).unwrap();

        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
        assert_eq!(loaded.dimension(), 2);
        assert_eq!(loaded.metric(), VectorMetric::Cosine);
        assert_eq!(loaded.vector_encoding(), VectorEncoding::I8ScalarQuantized);
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
    fn validation_rejects_same_size_corruption_in_every_snapshot_payload() {
        for file_name in [
            VECTORS_FILE,
            CHUNKS_FILE,
            RECORDS_FILE,
            BM25_FILE,
            TOMBSTONES_FILE,
        ] {
            let directory = temp_index_dir(&format!("checksum-{file_name}"));
            let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
            index
                .upsert_document(
                    document("doc-1"),
                    vec![chunk_input("checksum data", vec![1.0, 0.0])],
                )
                .unwrap();
            index.save_to_dir(&directory).unwrap();

            let manifest: PersistedManifest =
                read_json_file(&directory.join(MANIFEST_FILE)).unwrap();
            let path = manifest.data_directory(&directory).unwrap().join(file_name);
            let mut bytes = read_file(&path).unwrap();
            bytes[0] ^= 0xff;
            write_file(&path, &bytes).unwrap();

            let error = ExactVectorIndex::validate_dir(&directory).unwrap_err();
            assert!(matches!(error, RetrievalKitError::CorruptIndex { .. }));
            assert!(error.to_string().contains("SHA-256 checksum mismatch"));
            assert!(error.to_string().contains(file_name));

            let _ = fs::remove_dir_all(directory);
        }
    }

    #[test]
    fn validation_rejects_truncated_and_appended_snapshot_payloads() {
        for (file_name, append) in [
            (VECTORS_FILE, false),
            (CHUNKS_FILE, false),
            (RECORDS_FILE, false),
            (BM25_FILE, false),
            (TOMBSTONES_FILE, false),
            (VECTORS_FILE, true),
            (CHUNKS_FILE, true),
            (RECORDS_FILE, true),
            (BM25_FILE, true),
            (TOMBSTONES_FILE, true),
        ] {
            let directory = temp_index_dir(&format!("size-{file_name}-{append}"));
            let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
            index
                .upsert_document(
                    document("doc-1"),
                    vec![chunk_input("size data", vec![1.0, 0.0])],
                )
                .unwrap();
            index.save_to_dir(&directory).unwrap();

            let manifest: PersistedManifest =
                read_json_file(&directory.join(MANIFEST_FILE)).unwrap();
            let path = manifest.data_directory(&directory).unwrap().join(file_name);
            let mut bytes = read_file(&path).unwrap();
            if append {
                bytes.push(0);
            } else {
                bytes.pop();
            }
            write_file(&path, &bytes).unwrap();

            let error = ExactVectorIndex::validate_dir(&directory).unwrap_err();
            assert!(error.to_string().contains("manifest"));
            assert!(error.to_string().contains(file_name));

            let _ = fs::remove_dir_all(directory);
        }
    }

    #[test]
    fn validation_rejects_malformed_manifest_checksum() {
        let directory = temp_index_dir("malformed-manifest-checksum");
        let index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index.save_to_dir(&directory).unwrap();

        let manifest_path = directory.join(MANIFEST_FILE);
        let mut manifest: PersistedManifest = read_json_file(&manifest_path).unwrap();
        manifest.checksums.as_mut().unwrap().vectors = "not-a-sha256".to_owned();
        write_json_file(&manifest_path, &manifest).unwrap();

        let error = ExactVectorIndex::validate_dir(&directory).unwrap_err();
        assert!(matches!(error, RetrievalKitError::InvalidFormat { .. }));
        assert!(error.to_string().contains("64 lowercase hex characters"));

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn validation_does_not_clean_or_modify_index_directory() {
        let directory = temp_index_dir("validation-is-read-only");
        let index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index.save_to_dir(&directory).unwrap();

        let abandoned_snapshot = directory.join(SNAPSHOTS_DIRECTORY).join("abandoned");
        fs::create_dir(&abandoned_snapshot).unwrap();
        let temporary_manifest = directory.join("manifest.abandoned.tmp");
        write_file(&temporary_manifest, b"unfinished").unwrap();
        let manifest_before = read_file(&directory.join(MANIFEST_FILE)).unwrap();

        ExactVectorIndex::validate_dir(&directory).unwrap();

        assert!(abandoned_snapshot.exists());
        assert!(temporary_manifest.exists());
        assert_eq!(
            read_file(&directory.join(MANIFEST_FILE)).unwrap(),
            manifest_before
        );

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn validation_rejects_non_boolean_tombstone_bytes_in_v2_indexes() {
        let directory = temp_index_dir("invalid-tombstone-byte");
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("active", vec![1.0, 0.0])],
            )
            .unwrap();
        index.save_to_dir(&directory).unwrap();

        let manifest_path = directory.join(MANIFEST_FILE);
        let mut manifest: PersistedManifest = read_json_file(&manifest_path).unwrap();
        let tombstones_path = manifest
            .data_directory(&directory)
            .unwrap()
            .join(TOMBSTONES_FILE);
        manifest.format_version = TRANSACTIONAL_FORMAT_VERSION;
        manifest.checksums = None;
        write_json_file(&manifest_path, &manifest).unwrap();
        write_file(&tombstones_path, &[2]).unwrap();

        let error = ExactVectorIndex::validate_dir(&directory).unwrap_err();
        assert!(matches!(error, RetrievalKitError::CorruptIndex { .. }));
        assert!(error.to_string().contains("expected 0 or 1"));

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn loader_accepts_transactional_v2_manifest_without_checksums() {
        let directory = temp_index_dir("v2-without-checksums");
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("v2 data", vec![1.0, 0.0])],
            )
            .unwrap();
        index.save_to_dir(&directory).unwrap();

        let manifest_path = directory.join(MANIFEST_FILE);
        let mut manifest: PersistedManifest = read_json_file(&manifest_path).unwrap();
        manifest.format_version = TRANSACTIONAL_FORMAT_VERSION;
        manifest.checksums = None;
        write_json_file(&manifest_path, &manifest).unwrap();

        ExactVectorIndex::validate_dir(&directory).unwrap();
        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
        assert_eq!(loaded.chunk(0).unwrap().text, "v2 data");

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn failed_snapshot_save_preserves_previous_generation_at_every_checkpoint() {
        let directory = temp_index_dir("transactional-save-failures");
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("previous generation", vec![1.0, 0.0])],
            )
            .unwrap();
        index.save_to_dir(&directory).unwrap();
        let original_manifest = read_file(&directory.join(MANIFEST_FILE)).unwrap();

        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("replacement generation", vec![0.0, 1.0])],
            )
            .unwrap();

        for failure_checkpoint in [
            SaveCheckpoint::VectorsWritten,
            SaveCheckpoint::ChunksWritten,
            SaveCheckpoint::RecordsWritten,
            SaveCheckpoint::Bm25Written,
            SaveCheckpoint::TombstonesWritten,
            SaveCheckpoint::SnapshotSynced,
            SaveCheckpoint::ManifestWritten,
        ] {
            let result = index.save_to_dir_with_checkpoints(
                &directory,
                IndexPersistenceOptions::default(),
                |checkpoint| {
                    if checkpoint == failure_checkpoint {
                        Err(RetrievalKitError::Persistence {
                            operation: format!("simulate failure after {checkpoint:?}"),
                            path: directory.display().to_string(),
                            cause: "injected test failure".to_owned(),
                        })
                    } else {
                        Ok(())
                    }
                },
            );

            assert!(result.is_err());
            assert_eq!(
                read_file(&directory.join(MANIFEST_FILE)).unwrap(),
                original_manifest
            );
            let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded.chunk(0).unwrap().text, "previous generation");
        }

        index.save_to_dir(&directory).unwrap();
        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.chunk(1).unwrap().text, "replacement generation");
        assert_eq!(
            fs::read_dir(directory.join(SNAPSHOTS_DIRECTORY))
                .unwrap()
                .count(),
            1
        );
        assert!(fs::read_dir(&directory).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            !(name.starts_with("manifest.") && name.ends_with(".tmp"))
        }));

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn concurrent_save_is_rejected_without_changing_published_generation() {
        let directory = temp_index_dir("concurrent-save");
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("published", vec![1.0, 0.0])],
            )
            .unwrap();
        index.save_to_dir(&directory).unwrap();
        let original_manifest = read_file(&directory.join(MANIFEST_FILE)).unwrap();

        let lock = SaveLock::acquire(&directory).unwrap();
        let error = index.save_to_dir(&directory).unwrap_err();
        assert!(error
            .to_string()
            .contains("another save may already be running"));
        assert_eq!(
            read_file(&directory.join(MANIFEST_FILE)).unwrap(),
            original_manifest
        );
        drop(lock);

        index.save_to_dir(&directory).unwrap();
        ExactVectorIndex::load_from_dir(&directory).unwrap();

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn loader_accepts_legacy_root_file_layout() {
        let directory = temp_index_dir("legacy-layout");
        let mut index = ExactVectorIndex::new(2, VectorMetric::Cosine);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("legacy data", vec![1.0, 0.0])],
            )
            .unwrap();
        index.save_to_dir(&directory).unwrap();

        let manifest_path = directory.join(MANIFEST_FILE);
        let mut manifest: PersistedManifest = read_json_file(&manifest_path).unwrap();
        let snapshot_directory = manifest.data_directory(&directory).unwrap();
        for file_name in [
            VECTORS_FILE,
            CHUNKS_FILE,
            RECORDS_FILE,
            BM25_FILE,
            TOMBSTONES_FILE,
        ] {
            fs::rename(
                snapshot_directory.join(file_name),
                directory.join(file_name),
            )
            .unwrap();
        }
        manifest.format_version = LEGACY_FORMAT_VERSION;
        manifest.snapshot_id = None;
        manifest.checksums = None;
        write_json_file(&manifest_path, &manifest).unwrap();
        fs::remove_dir_all(directory.join(SNAPSHOTS_DIRECTORY)).unwrap();

        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
        assert_eq!(loaded.chunk(0).unwrap().text, "legacy data");
        assert_eq!(
            ExactVectorIndex::persisted_file_sizes(&directory)
                .unwrap()
                .tombstones_bytes,
            1
        );

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn saved_index_without_bm25_round_trips_vector_filter_and_tombstones() {
        let directory = temp_index_dir("vector-only-round-trip");
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

        let file_sizes = index
            .save_to_dir_with_options(&directory, IndexPersistenceOptions::vector_only())
            .unwrap();
        assert_eq!(file_sizes.bm25_bytes, 0);
        assert!(!directory.join(BM25_FILE).exists());
        assert_eq!(
            ExactVectorIndex::persisted_file_sizes(&directory).unwrap(),
            file_sizes
        );
        let manifest: PersistedManifest = read_json_file(&directory.join(MANIFEST_FILE)).unwrap();
        assert_eq!(manifest.chunk_compression, FileCompression::Zstd);
        assert_eq!(manifest.bm25_compression, FileCompression::None);
        assert_eq!(manifest.bm25_uncompressed_bytes, 0);

        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();
        assert_eq!(loaded.active_chunk_count(), 1);
        assert!(loaded.chunk(1).unwrap().deleted);

        let vector_hits = loaded
            .search(&SearchQuery::new(vec![1.0, 0.0], 10))
            .unwrap();
        assert_eq!(vector_hits.len(), 1);
        assert_eq!(vector_hits[0].chunk_id, 0);

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

        let keyword_hits = loaded
            .keyword_search(&KeywordQuery::new("swift local", 10))
            .unwrap();
        assert_eq!(keyword_hits.len(), 1);
        assert_eq!(keyword_hits[0].chunk_id, 0);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn saved_index_round_trips_hybrid_search() {
        let directory = temp_index_dir("hybrid-round-trip");
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("swift local search", vec![2.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-2"),
                vec![chunk_input("rust vector core", vec![0.0, 1.0])],
            )
            .unwrap();

        index.save_to_dir(&directory).unwrap();
        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();

        let hits = loaded
            .hybrid_search(&HybridQuery::new("swift search", vec![1.0, 0.0], 10))
            .unwrap();

        let shared_hit = hits.iter().find(|hit| hit.document_id == "doc-1").unwrap();
        assert_eq!(shared_hit.trace.vector_rank, Some(1));
        assert_eq!(shared_hit.trace.keyword_rank, Some(1));
        assert!(shared_hit.vector_score.is_some());
        assert!(shared_hit.keyword_score.is_some());
        assert_eq!(shared_hit.trace.matched_terms, vec!["search", "swift"]);
        assert_eq!(
            shared_hit.trace.fusion,
            HybridFusionTrace::WeightedNormalizedScore {
                vector_weight: 0.6,
                keyword_weight: 0.4
            }
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn saved_index_without_bm25_rebuilds_full_hybrid_state() {
        let directory = temp_index_dir("hybrid-vector-only-round-trip");
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("swift local search", vec![2.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-2"),
                vec![chunk_input("rust vector core", vec![0.0, 1.0])],
            )
            .unwrap();

        index
            .save_to_dir_with_options(&directory, IndexPersistenceOptions::vector_only())
            .unwrap();
        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();

        let hits = loaded
            .hybrid_search(&HybridQuery::new("swift search", vec![1.0, 0.0], 10))
            .unwrap();

        let vector_hit = hits.iter().find(|hit| hit.document_id == "doc-1").unwrap();
        assert_eq!(vector_hit.trace.vector_rank, Some(1));
        assert_eq!(vector_hit.trace.keyword_rank, Some(1));
        assert!(vector_hit.vector_score.is_some());
        assert!(vector_hit.keyword_score.is_some());
        assert!(vector_hit.trace.normalized_vector_score.is_some());
        assert!(vector_hit.trace.normalized_keyword_score.is_some());
        assert_eq!(vector_hit.trace.matched_terms, vec!["search", "swift"]);

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
    fn saved_i8_index_round_trips_hybrid_search() {
        let directory = temp_index_dir("i8-hybrid-round-trip");
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::DotProduct)
                .with_vector_encoding(VectorEncoding::I8ScalarQuantized),
        )
        .unwrap();
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("swift local search", vec![2.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-2"),
                vec![chunk_input("rust vector core", vec![0.0, 1.0])],
            )
            .unwrap();

        index.save_to_dir(&directory).unwrap();
        let loaded = ExactVectorIndex::load_from_dir(&directory).unwrap();

        assert_eq!(loaded.vector_encoding(), VectorEncoding::I8ScalarQuantized);
        let hits = loaded
            .hybrid_search(&HybridQuery::new("swift search", vec![1.0, 0.0], 10))
            .unwrap();

        let shared_hit = hits.iter().find(|hit| hit.document_id == "doc-1").unwrap();
        assert_eq!(shared_hit.trace.vector_rank, Some(1));
        assert_eq!(shared_hit.trace.keyword_rank, Some(1));
        assert!(shared_hit.vector_score.is_some());
        assert!(shared_hit.keyword_score.is_some());

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
    fn i8_unfiltered_fast_path_keeps_stable_top_k_ordering() {
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::DotProduct)
                .with_vector_encoding(VectorEncoding::I8ScalarQuantized),
        )
        .unwrap();
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
    fn i8_filtered_fast_path_applies_metadata_filter() {
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::DotProduct)
                .with_vector_encoding(VectorEncoding::I8ScalarQuantized),
        )
        .unwrap();
        let mut notes_chunk = chunk(1, "doc-1", vec![10.0, 0.0]);
        notes_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index.add_chunk(notes_chunk).unwrap();

        let mut transcript_chunk = chunk(2, "doc-2", vec![1.0, 0.0]);
        transcript_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index.add_chunk(transcript_chunk).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("transcript".to_owned()),
        });
        let hits = index.search(&query).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 2);
    }

    #[test]
    fn i8_filtered_fast_path_handles_full_scan_filter_fallback() {
        let mut index = ExactVectorIndex::try_with_config(
            IndexConfig::new(2, VectorMetric::DotProduct)
                .with_vector_encoding(VectorEncoding::I8ScalarQuantized),
        )
        .unwrap();
        let mut notes_chunk = chunk(1, "doc-1", vec![10.0, 0.0]);
        notes_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index.add_chunk(notes_chunk).unwrap();

        let mut transcript_chunk = chunk(2, "doc-2", vec![1.0, 0.0]);
        transcript_chunk.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index.add_chunk(transcript_chunk).unwrap();

        let query = SearchQuery::new(vec![1.0, 0.0], 10).with_filter(Filter::All(vec![
            Filter::Any(vec![
                Filter::Range {
                    field: "score".to_owned(),
                    lower: Some(MetadataValue::Float(f64::NAN)),
                    upper: None,
                },
                Filter::Equals {
                    field: "source".to_owned(),
                    value: MetadataValue::String("transcript".to_owned()),
                },
            ]),
            Filter::Exists {
                field: "source".to_owned(),
            },
        ]));
        let hits = index.search(&query).unwrap();

        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![2]
        );
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
            RetrievalKitError::UnsupportedVectorEncoding {
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
                vec![chunk_input(
                    "shared shared shared rare token",
                    vec![1.0, 0.0],
                )],
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
    fn hybrid_search_merges_shared_candidates_with_trace() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("swift local search", vec![2.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-2"),
                vec![chunk_input("rust vector core", vec![0.0, 1.0])],
            )
            .unwrap();

        let hits = index
            .hybrid_search(&HybridQuery::new("swift search", vec![1.0, 0.0], 10))
            .unwrap();

        let shared_hit = hits.iter().find(|hit| hit.document_id == "doc-1").unwrap();
        assert_eq!(shared_hit.vector_score, Some(2.0));
        assert!(shared_hit.keyword_score.is_some());
        assert_eq!(shared_hit.trace.vector_rank, Some(1));
        assert_eq!(shared_hit.trace.keyword_rank, Some(1));
        assert_eq!(shared_hit.trace.matched_terms, vec!["search", "swift"]);
        assert_eq!(
            shared_hit.trace.fusion,
            HybridFusionTrace::WeightedNormalizedScore {
                vector_weight: 0.6,
                keyword_weight: 0.4
            }
        );
        assert!(shared_hit.trace.normalized_vector_score.is_some());
        assert!(shared_hit.trace.normalized_keyword_score.is_some());
    }

    #[test]
    fn hybrid_search_returns_vector_only_candidates() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-vector"),
                vec![chunk_input("semantic only", vec![3.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword"),
                vec![chunk_input("rare keyword", vec![0.0, 1.0])],
            )
            .unwrap();

        let hits = index
            .hybrid_search(&HybridQuery::new("rare keyword", vec![1.0, 0.0], 10))
            .unwrap();

        let vector_hit = hits
            .iter()
            .find(|hit| hit.document_id == "doc-vector")
            .unwrap();
        assert_eq!(vector_hit.trace.vector_rank, Some(1));
        assert_eq!(vector_hit.trace.keyword_rank, None);
        assert_eq!(vector_hit.keyword_score, None);
        assert!(vector_hit.trace.matched_terms.is_empty());
    }

    #[test]
    fn hybrid_search_returns_keyword_only_candidates() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-vector"),
                vec![chunk_input("semantic only", vec![3.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword"),
                vec![chunk_input("rare keyword", vec![0.0, 1.0])],
            )
            .unwrap();

        let query =
            HybridQuery::new("rare keyword", vec![1.0, 0.0], 10).with_candidate_limits(1, 10);
        let hits = index.hybrid_search(&query).unwrap();

        let keyword_hit = hits
            .iter()
            .find(|hit| hit.document_id == "doc-keyword")
            .unwrap();
        assert_eq!(keyword_hit.trace.vector_rank, None);
        assert_eq!(keyword_hit.trace.keyword_rank, Some(1));
        assert_eq!(keyword_hit.vector_score, None);
        assert!(keyword_hit.keyword_score.is_some());
        assert_eq!(keyword_hit.trace.matched_terms, vec!["keyword", "rare"]);
    }

    #[test]
    fn hybrid_search_respects_candidate_limits_before_fusion() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-vector-1"),
                vec![chunk_input("semantic only", vec![3.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-vector-2"),
                vec![chunk_input("semantic only", vec![2.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword-1"),
                vec![chunk_input("alpha alpha alpha", vec![0.0, 1.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword-2"),
                vec![chunk_input("alpha", vec![0.0, 0.5])],
            )
            .unwrap();

        let vector_limited = index
            .hybrid_search(
                &HybridQuery::new("missing", vec![1.0, 0.0], 10).with_candidate_limits(1, 0),
            )
            .unwrap();

        assert_eq!(vector_limited.len(), 1);
        assert_eq!(vector_limited[0].document_id, "doc-vector-1");
        assert_eq!(vector_limited[0].trace.vector_rank, Some(1));
        assert_eq!(vector_limited[0].trace.keyword_rank, None);

        let keyword_limited = index
            .hybrid_search(
                &HybridQuery::new("alpha", vec![1.0, 0.0], 10).with_candidate_limits(0, 1),
            )
            .unwrap();

        assert_eq!(keyword_limited.len(), 1);
        assert_eq!(keyword_limited[0].document_id, "doc-keyword-1");
        assert_eq!(keyword_limited[0].trace.vector_rank, None);
        assert_eq!(keyword_limited[0].trace.keyword_rank, Some(1));
    }

    #[test]
    fn alpha_endpoints_disable_the_zero_weight_candidate_source() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-vector"),
                vec![chunk_input("semantic only", vec![3.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword-1"),
                vec![chunk_input("rare keyword rare", vec![0.0, 1.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword-2"),
                vec![chunk_input("rare keyword", vec![0.0, 0.5])],
            )
            .unwrap();

        let vector_only = index
            .hybrid_search(
                &HybridQuery::new("rare keyword", vec![1.0, 0.0], 10)
                    .with_candidate_limits(1, 10)
                    .try_with_alpha(1.0)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(vector_only.len(), 1);
        assert_eq!(vector_only[0].document_id, "doc-vector");
        assert_eq!(vector_only[0].keyword_score, None);
        assert_eq!(vector_only[0].trace.keyword_rank, None);
        assert_eq!(vector_only[0].trace.normalized_keyword_score, None);
        assert!(vector_only[0].trace.matched_terms.is_empty());

        let keyword_only = index
            .hybrid_search(
                &HybridQuery::new("rare keyword", Vec::new(), 10)
                    .with_candidate_limits(1, 10)
                    .try_with_alpha(0.0)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            keyword_only
                .iter()
                .map(|hit| hit.document_id.as_str())
                .collect::<Vec<_>>(),
            vec!["doc-keyword-1", "doc-keyword-2"]
        );
        assert!(keyword_only.iter().all(|hit| hit.vector_score.is_none()));
        assert!(keyword_only
            .iter()
            .all(|hit| hit.trace.vector_rank.is_none()
                && hit.trace.normalized_vector_score.is_none()));
    }

    #[test]
    fn hybrid_search_rrf_score_matches_ranks() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("swift local search", vec![2.0, 0.0])],
            )
            .unwrap();

        let query = HybridQuery::new("swift search", vec![1.0, 0.0], 10).with_rrf_k(60.0);
        let hits = index.hybrid_search(&query).unwrap();
        let hit = &hits[0];

        let expected = 1.0 / (60.0 + hit.trace.vector_rank.unwrap() as f32)
            + 1.0 / (60.0 + hit.trace.keyword_rank.unwrap() as f32);
        assert_close(hit.score, expected);
    }

    #[test]
    fn hybrid_search_weighted_normalized_score_can_prioritize_keyword_hits() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-vector"),
                vec![chunk_input("semantic only", vec![3.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword"),
                vec![chunk_input("rare keyword", vec![0.0, 1.0])],
            )
            .unwrap();

        let query = HybridQuery::new("rare keyword", vec![1.0, 0.0], 10)
            .with_weighted_normalized_score(0.25, 0.75);
        let hits = index.hybrid_search(&query).unwrap();

        assert_eq!(hits[0].document_id, "doc-keyword");
        assert_eq!(
            hits[0].trace.fusion,
            HybridFusionTrace::WeightedNormalizedScore {
                vector_weight: 0.25,
                keyword_weight: 0.75
            }
        );
        assert_eq!(hits[0].trace.normalized_vector_score, Some(0.0));
        assert_eq!(hits[0].trace.normalized_keyword_score, Some(1.0));
        assert_close(hits[0].score, 0.75);

        assert_eq!(hits[1].document_id, "doc-vector");
        assert_eq!(hits[1].trace.normalized_vector_score, Some(1.0));
        assert_eq!(hits[1].trace.normalized_keyword_score, None);
        assert_close(hits[1].score, 0.25);
    }

    #[test]
    fn hybrid_search_weighted_normalized_score_can_prioritize_vector_hits() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-vector"),
                vec![chunk_input("semantic only", vec![3.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-keyword"),
                vec![chunk_input("rare keyword", vec![0.0, 1.0])],
            )
            .unwrap();

        let query = HybridQuery::new("rare keyword", vec![1.0, 0.0], 10)
            .with_weighted_normalized_score(0.75, 0.25);
        let hits = index.hybrid_search(&query).unwrap();

        assert_eq!(hits[0].document_id, "doc-vector");
        assert_close(hits[0].score, 0.75);
        assert_eq!(hits[1].document_id, "doc-keyword");
        assert_close(hits[1].score, 0.25);
    }

    #[test]
    fn hybrid_search_rejects_invalid_weighted_normalized_scores() {
        let index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let query = HybridQuery::new("anything", vec![1.0, 0.0], 10)
            .with_weighted_normalized_score(0.0, 0.0);

        let error = index.hybrid_search(&query).unwrap_err();

        assert_eq!(
            error,
            RetrievalKitError::InvalidFormat {
                message: "at least one hybrid fusion weight must be greater than zero".to_owned()
            }
        );
    }

    #[test]
    fn hybrid_search_applies_metadata_filters_to_both_modes() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let mut vector_document = document("doc-vector-excluded");
        vector_document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index
            .upsert_document(
                vector_document,
                vec![chunk_input("semantic only", vec![3.0, 0.0])],
            )
            .unwrap();

        let mut keyword_document = document("doc-keyword-excluded");
        keyword_document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("notes".to_owned()),
        );
        index
            .upsert_document(
                keyword_document,
                vec![chunk_input("rare keyword", vec![0.0, 1.0])],
            )
            .unwrap();

        let mut allowed_document = document("doc-allowed");
        allowed_document.metadata.insert(
            "source".to_owned(),
            MetadataValue::String("transcript".to_owned()),
        );
        index
            .upsert_document(
                allowed_document,
                vec![chunk_input("rare keyword", vec![0.0, 0.5])],
            )
            .unwrap();

        let query =
            HybridQuery::new("rare keyword", vec![1.0, 0.0], 10).with_filter(Filter::Equals {
                field: "source".to_owned(),
                value: MetadataValue::String("transcript".to_owned()),
            });
        let hits = index.hybrid_search(&query).unwrap();

        assert_eq!(
            hits.iter()
                .map(|hit| hit.document_id.as_str())
                .collect::<Vec<_>>(),
            vec!["doc-allowed"]
        );
    }

    #[test]
    fn hybrid_search_excludes_deleted_documents() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-deleted"),
                vec![chunk_input("rare keyword", vec![3.0, 0.0])],
            )
            .unwrap();
        index
            .upsert_document(
                document("doc-active"),
                vec![chunk_input("active fallback", vec![0.0, 1.0])],
            )
            .unwrap();
        assert_eq!(index.delete_document("doc-deleted"), 1);

        let hits = index
            .hybrid_search(&HybridQuery::new("rare keyword", vec![1.0, 0.0], 10))
            .unwrap();

        assert!(!hits.iter().any(|hit| hit.document_id == "doc-deleted"));
    }

    #[test]
    fn hybrid_search_excludes_superseded_chunks() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let old_chunk_ids = index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("old codename alpha", vec![3.0, 0.0])],
            )
            .unwrap();
        let new_chunk_ids = index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("new codename beta", vec![0.0, 1.0])],
            )
            .unwrap();

        let hits = index
            .hybrid_search(&HybridQuery::new("old alpha", vec![1.0, 0.0], 10))
            .unwrap();

        assert!(!hits.iter().any(|hit| old_chunk_ids.contains(&hit.chunk_id)));
        assert!(hits.iter().any(|hit| new_chunk_ids.contains(&hit.chunk_id)));
        for hit in hits {
            let chunk = index.chunk(hit.chunk_id).unwrap();
            assert_ne!(chunk.text, "old codename alpha");
        }
    }

    #[test]
    fn hybrid_search_top_k_zero_returns_empty() {
        let mut index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        index
            .upsert_document(
                document("doc-1"),
                vec![chunk_input("rare keyword", vec![1.0, 0.0])],
            )
            .unwrap();

        let hits = index
            .hybrid_search(&HybridQuery::new("rare keyword", vec![1.0, 0.0], 0))
            .unwrap();

        assert!(hits.is_empty());
    }

    #[test]
    fn hybrid_search_dimension_mismatch_returns_error() {
        let index = ExactVectorIndex::new(3, VectorMetric::DotProduct);
        let query = HybridQuery::new("anything", vec![1.0, 0.0], 10);

        let error = index.hybrid_search(&query).unwrap_err();

        assert_eq!(
            error,
            RetrievalKitError::InvalidDimension {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn hybrid_search_rejects_invalid_rrf_k() {
        let index = ExactVectorIndex::new(2, VectorMetric::DotProduct);
        let query = HybridQuery::new("anything", vec![1.0, 0.0], 10).with_rrf_k(0.0);

        let error = index.hybrid_search(&query).unwrap_err();

        assert_eq!(
            error,
            RetrievalKitError::InvalidFormat {
                message: "rrf_k must be finite and greater than zero".to_owned()
            }
        );
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
        assert_eq!(index.corpus.active_offsets, vec![1]);
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
        assert!(index.corpus.active_offsets.is_empty());
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
        assert!(index.corpus.active_offsets.is_empty());
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
            RetrievalKitError::InvalidDimension {
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
