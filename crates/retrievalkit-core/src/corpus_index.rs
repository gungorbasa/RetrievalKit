use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::candidate_scope::CandidateScope;
use crate::error::{Result, RetrievalKitError};
use crate::filter::Filter;
use crate::metadata::Metadata;
use crate::record_store::{
    ChunkIdentity, ChunkKey, CorpusId, GenerationId, Record, RecordId, RecordStore,
};
use crate::types::{ChunkId, StoredChunk};

/// Capability-neutral retrievable chunk supplied with a canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct CorpusChunkInput {
    pub key: ChunkKey,
    pub text: String,
    pub metadata: Metadata,
}

/// One canonical record and its capability-neutral retrievable chunks.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordInput {
    pub record: Record,
    pub metadata: Metadata,
    pub chunks: Vec<CorpusChunkInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedCorpus {
    corpus_id: CorpusId,
    generation: GenerationId,
    record_store: RecordStore,
    chunk_identities: Vec<(ChunkIdentity, ChunkId)>,
    chunks: Vec<StoredChunk>,
    next_chunk_id: ChunkId,
}

/// Canonical graph-neutral corpus state shared by derived query capabilities.
///
/// `CorpusIndex` is the sole owner of source records, retrievable chunks,
/// stable external-to-internal identity mappings, lifecycle versions, and the
/// active generation. Vector, lexical, and graph structures are derived from
/// this state and must not become independently editable payload owners.
#[derive(Debug, Clone)]
pub struct CorpusIndex {
    pub(crate) corpus_id: CorpusId,
    pub(crate) generation: GenerationId,
    pub(crate) record_store: RecordStore,
    pub(crate) chunk_ids_by_identity: BTreeMap<ChunkIdentity, ChunkId>,
    pub(crate) chunk_identities: BTreeMap<ChunkId, ChunkIdentity>,
    pub(crate) chunks: Vec<StoredChunk>,
    pub(crate) chunk_offsets: Vec<Option<usize>>,
    pub(crate) active_offsets: Vec<usize>,
    pub(crate) next_chunk_id: ChunkId,
    pub(crate) record_versions: BTreeMap<String, u64>,
}

impl CorpusIndex {
    pub fn new(corpus_id: CorpusId) -> Self {
        Self {
            record_store: RecordStore::new(corpus_id.clone()),
            corpus_id,
            generation: GenerationId::INITIAL,
            chunk_ids_by_identity: BTreeMap::new(),
            chunk_identities: BTreeMap::new(),
            chunks: Vec::new(),
            chunk_offsets: Vec::new(),
            active_offsets: Vec::new(),
            next_chunk_id: 0,
            record_versions: BTreeMap::new(),
        }
    }

    pub fn corpus_id(&self) -> &CorpusId {
        &self.corpus_id
    }

    pub fn generation(&self) -> GenerationId {
        self.generation
    }

    pub fn record_store(&self) -> &RecordStore {
        &self.record_store
    }

    pub fn record(&self, record_id: &RecordId) -> Option<&Record> {
        self.record_store.get(record_id)
    }

    pub fn hydrate_records<'a>(&'a self, record_ids: &[RecordId]) -> Vec<Option<&'a Record>> {
        self.record_store.hydrate(record_ids)
    }

    pub fn chunk_id_for_identity(&self, identity: &ChunkIdentity) -> Option<ChunkId> {
        self.chunk_ids_by_identity.get(identity).copied()
    }

    pub fn chunk_identity(&self, chunk_id: ChunkId) -> Option<&ChunkIdentity> {
        self.chunk_identities.get(&chunk_id)
    }

    pub fn chunk_identities(&self) -> impl Iterator<Item = (&ChunkIdentity, ChunkId)> {
        self.chunk_ids_by_identity
            .iter()
            .map(|(identity, chunk_id)| (identity, *chunk_id))
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn active_chunk_count(&self) -> usize {
        self.active_offsets.len()
    }

    pub fn tombstoned_chunk_count(&self) -> usize {
        self.chunks.iter().filter(|chunk| chunk.deleted).count()
    }

    pub fn chunk(&self, chunk_id: ChunkId) -> Option<&StoredChunk> {
        let offset = self
            .chunk_offsets
            .get(usize::try_from(chunk_id).ok()?)?
            .as_ref()?;
        self.chunks.get(*offset)
    }

    pub fn hydrate_chunks<'a>(&'a self, chunk_ids: &[ChunkId]) -> Vec<Option<&'a StoredChunk>> {
        chunk_ids
            .iter()
            .map(|chunk_id| self.chunk(*chunk_id).filter(|chunk| !chunk.deleted))
            .collect()
    }

    pub fn candidate_scope(
        &self,
        chunk_ids: impl IntoIterator<Item = ChunkId>,
    ) -> Result<CandidateScope> {
        let mut chunk_ids = chunk_ids.into_iter().collect::<Vec<_>>();
        chunk_ids.sort_unstable();
        chunk_ids.dedup();
        for chunk_id in &chunk_ids {
            let Some(chunk) = self.chunk(*chunk_id) else {
                return Err(RetrievalKitError::InvalidCandidateScope {
                    chunk_id: *chunk_id,
                    message: "the ID is unavailable in this generation".to_owned(),
                });
            };
            if chunk.deleted {
                return Err(RetrievalKitError::InvalidCandidateScope {
                    chunk_id: *chunk_id,
                    message: "the ID is deleted or superseded in this generation".to_owned(),
                });
            }
        }
        Ok(CandidateScope::from_sorted_ids(
            self.corpus_id.clone(),
            self.generation,
            chunk_ids,
            self.chunk_offsets.len(),
        ))
    }

    pub fn candidate_scope_for_identities(
        &self,
        identities: impl IntoIterator<Item = ChunkIdentity>,
    ) -> Result<CandidateScope> {
        let chunk_ids = identities
            .into_iter()
            .map(|identity| {
                self.chunk_id_for_identity(&identity).ok_or_else(|| {
                    RetrievalKitError::InvalidIdentity {
                        kind: "ChunkIdentity",
                        value: format!(
                            "{}/{}",
                            identity.record_id.as_str(),
                            identity.chunk_key.as_str()
                        ),
                        message: "is unavailable in the active generation".to_owned(),
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.candidate_scope(chunk_ids)
    }

    /// Applies a metadata filter while preserving this scope's corpus and generation.
    ///
    /// The scope is validated even when `filter` is `None`. Every retained
    /// member is active in the canonical corpus at this generation.
    pub fn filter_candidate_scope(
        &self,
        scope: &CandidateScope,
        filter: Option<&Filter>,
    ) -> Result<CandidateScope> {
        self.validate_candidate_scope(scope)?;
        let Some(filter) = filter else {
            return Ok(scope.clone());
        };

        let mut chunk_ids = Vec::with_capacity(scope.len());
        for chunk_id in scope.ids() {
            let chunk = self.validate_candidate_scope_member(chunk_id)?;
            if filter.matches(&chunk.metadata)? {
                chunk_ids.push(chunk_id);
            }
        }
        Ok(CandidateScope::from_sorted_ids(
            self.corpus_id.clone(),
            self.generation,
            chunk_ids,
            self.chunk_offsets.len(),
        ))
    }

    /// Materializes stable external identities in lexical identity order.
    pub fn candidate_scope_identities(&self, scope: &CandidateScope) -> Result<Vec<ChunkIdentity>> {
        self.validate_candidate_scope(scope)?;
        let mut identities = Vec::with_capacity(scope.len());
        for chunk_id in scope.ids() {
            let chunk = self.validate_candidate_scope_member(chunk_id)?;
            let Some(identity) = self.chunk_identities.get(&chunk_id) else {
                return Err(RetrievalKitError::InvalidCandidateScope {
                    chunk_id,
                    message: "the stable external identity is unavailable".to_owned(),
                });
            };
            if chunk.document_id != identity.record_id.as_str()
                || self.chunk_ids_by_identity.get(identity) != Some(&chunk_id)
                || self.record_store.get(&identity.record_id).is_none()
            {
                return Err(RetrievalKitError::InvalidCandidateScope {
                    chunk_id,
                    message: "the stable external identity is inconsistent".to_owned(),
                });
            }
            identities.push(identity.clone());
        }
        identities.sort();
        identities.dedup();
        Ok(identities)
    }

    /// Adds or atomically replaces one canonical record without constructing
    /// retrieval state. This is the graph-only ingestion path.
    pub fn upsert(&mut self, input: RecordInput) -> Result<Vec<ChunkId>> {
        input.record.validate()?;
        let mut keys = BTreeSet::new();
        for chunk in &input.chunks {
            if !keys.insert(chunk.key.clone()) {
                return Err(RetrievalKitError::InvalidIdentity {
                    kind: "ChunkKey",
                    value: chunk.key.as_str().to_owned(),
                    message: "must be unique within one record generation".to_owned(),
                });
            }
        }

        let record_id = input.record.id.clone();
        let document_id = record_id.as_str().to_owned();
        let version = self.record_versions.get(&document_id).copied().unwrap_or(0) + 1;
        let mut deactivated_offsets = Vec::new();
        for (offset, chunk) in self.chunks.iter_mut().enumerate() {
            if chunk.document_id == document_id && !chunk.deleted {
                chunk.deleted = true;
                deactivated_offsets.push(offset);
            }
        }
        self.remove_active_offsets(&deactivated_offsets);
        self.remove_chunk_identities_for_record(&record_id);

        let mut chunk_ids = Vec::with_capacity(input.chunks.len());
        for chunk in input.chunks {
            let offset = self.chunks.len();
            let chunk_id = self.allocate_chunk_id();
            let identity = ChunkIdentity::new(record_id.clone(), chunk.key);
            let stored_chunk = StoredChunk {
                chunk_id,
                document_id: document_id.clone(),
                text: chunk.text,
                metadata: merge_metadata(&input.metadata, chunk.metadata),
                deleted: false,
                version,
            };
            self.register_chunk_offset(chunk_id, offset);
            self.chunk_ids_by_identity
                .insert(identity.clone(), chunk_id);
            self.chunk_identities.insert(chunk_id, identity);
            self.active_offsets.push(offset);
            self.chunks.push(stored_chunk);
            chunk_ids.push(chunk_id);
        }

        self.record_versions.insert(document_id, version);
        self.record_store.upsert(input.record)?;
        self.generation = self.generation.next();
        Ok(chunk_ids)
    }

    pub fn delete_record(&mut self, record_id: &RecordId) -> usize {
        let mut deleted = 0;
        let mut offsets = Vec::new();
        for (offset, chunk) in self.chunks.iter_mut().enumerate() {
            if chunk.document_id == record_id.as_str() && !chunk.deleted {
                chunk.deleted = true;
                offsets.push(offset);
                deleted += 1;
            }
        }
        self.remove_active_offsets(&offsets);
        let removed_record = self.record_store.delete(record_id);
        self.remove_chunk_identities_for_record(record_id);
        if deleted > 0 || removed_record.is_some() {
            self.generation = self.generation.next();
        }
        deleted
    }

    /// Encodes the canonical corpus without retrieval or graph payloads.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(&PersistedCorpus {
            corpus_id: self.corpus_id.clone(),
            generation: self.generation,
            record_store: self.record_store.clone(),
            chunk_identities: self
                .chunk_ids_by_identity
                .iter()
                .map(|(identity, chunk_id)| (identity.clone(), *chunk_id))
                .collect(),
            chunks: self.chunks.clone(),
            next_chunk_id: self.next_chunk_id,
        })
        .map_err(|error| RetrievalKitError::InvalidFormat {
            message: format!("could not encode canonical corpus snapshot: {error}"),
        })
    }

    /// Restores and validates a capability-neutral corpus snapshot.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self> {
        let persisted: PersistedCorpus =
            serde_json::from_slice(bytes).map_err(|error| RetrievalKitError::InvalidFormat {
                message: format!("could not decode canonical corpus snapshot: {error}"),
            })?;
        let mut corpus = Self::new(persisted.corpus_id);
        corpus.generation = persisted.generation;
        corpus.record_store = persisted.record_store;
        corpus.chunks = persisted.chunks;
        corpus.next_chunk_id = persisted.next_chunk_id;
        for (identity, chunk_id) in persisted.chunk_identities {
            if corpus
                .chunk_ids_by_identity
                .insert(identity.clone(), chunk_id)
                .is_some()
            {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!(
                        "canonical corpus repeats chunk identity {}/{}",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    ),
                });
            }
            corpus.chunk_identities.insert(chunk_id, identity);
        }
        corpus.rebuild_offsets_and_versions();
        corpus.validate()?;
        Ok(corpus)
    }

    pub(crate) fn validate_candidate_scope(&self, scope: &CandidateScope) -> Result<()> {
        if scope.corpus_id() == &self.corpus_id && scope.generation() == self.generation {
            return Ok(());
        }
        Err(RetrievalKitError::StaleGeneration {
            expected_corpus: self.corpus_id.as_str().to_owned(),
            expected_generation: self.generation.get(),
            actual_corpus: scope.corpus_id().as_str().to_owned(),
            actual_generation: scope.generation().get(),
        })
    }

    fn validate_candidate_scope_member(&self, chunk_id: ChunkId) -> Result<&StoredChunk> {
        let Some(chunk) = self.chunk(chunk_id) else {
            return Err(RetrievalKitError::InvalidCandidateScope {
                chunk_id,
                message: "the ID is unavailable in this generation".to_owned(),
            });
        };
        if chunk.deleted {
            return Err(RetrievalKitError::InvalidCandidateScope {
                chunk_id,
                message: "the ID is deleted or superseded in this generation".to_owned(),
            });
        }
        Ok(chunk)
    }

    pub(crate) fn allocate_chunk_id(&mut self) -> ChunkId {
        let chunk_id = self.next_chunk_id;
        self.next_chunk_id = self.next_chunk_id.saturating_add(1);
        chunk_id
    }

    pub(crate) fn register_chunk_offset(&mut self, chunk_id: ChunkId, offset: usize) {
        let Ok(chunk_id) = usize::try_from(chunk_id) else {
            return;
        };
        if self.chunk_offsets.len() <= chunk_id {
            self.chunk_offsets.resize(chunk_id + 1, None);
        }
        self.chunk_offsets[chunk_id] = Some(offset);
    }

    pub(crate) fn remove_active_offsets(&mut self, offsets: &[usize]) {
        if offsets.is_empty() {
            return;
        }
        self.active_offsets
            .retain(|active_offset| !offsets.contains(active_offset));
    }

    pub(crate) fn remove_chunk_identities_for_record(&mut self, record_id: &RecordId) {
        self.chunk_ids_by_identity
            .retain(|identity, _| &identity.record_id != record_id);
        self.chunk_identities
            .retain(|_, identity| &identity.record_id != record_id);
    }

    pub(crate) fn rebuild_offsets_and_versions(&mut self) {
        self.chunk_offsets.clear();
        self.active_offsets.clear();
        self.record_versions.clear();
        self.next_chunk_id = 0;

        for offset in 0..self.chunks.len() {
            let chunk_id = self.chunks[offset].chunk_id;
            self.next_chunk_id = self.next_chunk_id.max(chunk_id.saturating_add(1));
            self.register_chunk_offset(chunk_id, offset);

            let chunk = &self.chunks[offset];
            self.record_versions
                .entry(chunk.document_id.clone())
                .and_modify(|version| *version = (*version).max(chunk.version))
                .or_insert(chunk.version);
            if !chunk.deleted {
                self.active_offsets.push(offset);
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.record_store.corpus_id() != &self.corpus_id {
            return Err(RetrievalKitError::InvalidFormat {
                message: "canonical record corpus does not match the index corpus".to_owned(),
            });
        }
        self.record_store.validate()?;
        let mut seen_chunk_ids = BTreeSet::new();
        for (identity, chunk_id) in &self.chunk_ids_by_identity {
            if !seen_chunk_ids.insert(*chunk_id) {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!(
                        "multiple external chunk identities resolve to internal chunk ID {chunk_id}"
                    ),
                });
            }
            if self.record_store.get(&identity.record_id).is_none() {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!(
                        "chunk identity {}/{} references a missing canonical record",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    ),
                });
            }
            let Some(chunk) = self.chunk(*chunk_id) else {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!(
                        "chunk identity {}/{} references unavailable internal chunk ID {chunk_id}",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    ),
                });
            };
            if chunk.deleted || chunk.document_id != identity.record_id.as_str() {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!(
                        "chunk identity {}/{} does not resolve to its active canonical record",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    ),
                });
            }
            if self.chunk_identities.get(chunk_id) != Some(identity) {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!(
                        "reverse identity mapping is missing for internal chunk ID {chunk_id}"
                    ),
                });
            }
        }
        if self.chunk_identities.len() != self.chunk_ids_by_identity.len() {
            return Err(RetrievalKitError::InvalidFormat {
                message: "forward and reverse chunk identity mapping sizes differ".to_owned(),
            });
        }
        Ok(())
    }
}

fn merge_metadata(inherited: &Metadata, chunk: Metadata) -> Metadata {
    let mut metadata = inherited.clone();
    metadata.extend(chunk);
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Filter;
    use crate::metadata::MetadataValue;
    use crate::record_store::{ChunkKey, RecordType};

    fn record_input(record_id: &str, chunks: &[(&str, Metadata)]) -> RecordInput {
        RecordInput {
            record: Record {
                id: RecordId::new(record_id).unwrap(),
                record_type: RecordType::new("Topic").unwrap(),
                fields: BTreeMap::new(),
                content: None,
            },
            metadata: BTreeMap::new(),
            chunks: chunks
                .iter()
                .map(|(key, metadata)| CorpusChunkInput {
                    key: ChunkKey::new(*key).unwrap(),
                    text: format!("{record_id}-{key}"),
                    metadata: metadata.clone(),
                })
                .collect(),
        }
    }

    #[test]
    fn candidate_scopes_are_bound_to_the_canonical_generation() {
        let mut corpus = CorpusIndex::new(CorpusId::new("corpus").unwrap());
        corpus.chunks.push(StoredChunk {
            chunk_id: 0,
            document_id: "record".to_owned(),
            text: "text".to_owned(),
            metadata: BTreeMap::new(),
            deleted: false,
            version: 1,
        });
        corpus.register_chunk_offset(0, 0);
        corpus.active_offsets.push(0);
        let record_id = RecordId::new("record").unwrap();
        let identity = ChunkIdentity::new(record_id.clone(), ChunkKey::new("chunk").unwrap());
        corpus
            .record_store
            .upsert(Record {
                id: record_id,
                record_type: RecordType::new("Topic").unwrap(),
                fields: BTreeMap::new(),
                content: None,
            })
            .unwrap();
        corpus.chunk_ids_by_identity.insert(identity.clone(), 0);
        corpus.chunk_identities.insert(0, identity);

        let scope = corpus.candidate_scope([0]).unwrap();
        assert_eq!(scope.corpus_id(), corpus.corpus_id());
        assert_eq!(scope.generation(), corpus.generation());

        corpus.generation = corpus.generation.next();
        assert!(matches!(
            corpus.validate_candidate_scope(&scope),
            Err(RetrievalKitError::StaleGeneration { .. })
        ));
    }

    #[test]
    fn graph_only_ingestion_and_snapshot_need_no_embeddings() {
        let mut corpus = CorpusIndex::new(CorpusId::new("graph-only").unwrap());
        let chunk_ids = corpus
            .upsert(RecordInput {
                record: Record {
                    id: RecordId::new("rust").unwrap(),
                    record_type: RecordType::new("Topic").unwrap(),
                    fields: BTreeMap::new(),
                    content: None,
                },
                metadata: BTreeMap::new(),
                chunks: vec![CorpusChunkInput {
                    key: ChunkKey::new("summary").unwrap(),
                    text: "native retrieval".to_owned(),
                    metadata: BTreeMap::new(),
                }],
            })
            .unwrap();
        assert_eq!(chunk_ids, vec![0]);

        let loaded = CorpusIndex::from_snapshot_bytes(&corpus.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(loaded.record_store(), corpus.record_store());
        assert_eq!(loaded.chunk(0), corpus.chunk(0));
        assert_eq!(loaded.generation(), corpus.generation());
    }

    #[test]
    fn corpus_filters_candidate_scopes_with_every_filter_shape() {
        let mut corpus = CorpusIndex::new(CorpusId::new("filters").unwrap());
        let first = BTreeMap::from([
            ("team".to_owned(), MetadataValue::from("mobile")),
            ("rank".to_owned(), MetadataValue::from(1_i64)),
            ("active".to_owned(), MetadataValue::from(true)),
        ]);
        let second = BTreeMap::from([
            ("team".to_owned(), MetadataValue::from("platform")),
            ("rank".to_owned(), MetadataValue::from(2_i64)),
        ]);
        let third = BTreeMap::from([
            ("team".to_owned(), MetadataValue::from("mobile")),
            ("rank".to_owned(), MetadataValue::from(3_i64)),
        ]);
        corpus
            .upsert(record_input(
                "record",
                &[("z", first), ("a", second), ("m", third)],
            ))
            .unwrap();
        let scope = corpus.candidate_scope([2, 0, 1, 1]).unwrap();

        let cases = [
            (Filter::eq("team", "mobile"), 2),
            (Filter::ne("team", "mobile"), 1),
            (Filter::in_values("rank", [1_i64, 3]), 2),
            (Filter::between("rank", 2_i64, 3_i64), 2),
            (Filter::exists("active"), 1),
            (
                Filter::all([Filter::eq("team", "mobile"), Filter::gte("rank", 2_i64)]),
                1,
            ),
            (
                Filter::any([Filter::eq("team", "platform"), Filter::eq("rank", 3_i64)]),
                2,
            ),
        ];
        for (filter, expected) in cases {
            assert_eq!(
                corpus
                    .filter_candidate_scope(&scope, Some(&filter))
                    .unwrap()
                    .len(),
                expected
            );
        }

        assert_eq!(corpus.filter_candidate_scope(&scope, None).unwrap(), scope);
        assert!(corpus
            .filter_candidate_scope(
                &corpus.candidate_scope([]).unwrap(),
                Some(&Filter::exists("x"))
            )
            .unwrap()
            .is_empty());
    }

    #[test]
    fn candidate_identity_materialization_is_deduplicated_and_lexical() {
        let mut corpus = CorpusIndex::new(CorpusId::new("identities").unwrap());
        corpus
            .upsert(record_input(
                "z-record",
                &[("z", BTreeMap::new()), ("a", BTreeMap::new())],
            ))
            .unwrap();
        corpus
            .upsert(record_input("a-record", &[("m", BTreeMap::new())]))
            .unwrap();
        let scope = corpus.candidate_scope([0, 2, 1, 2]).unwrap();

        let identities = corpus.candidate_scope_identities(&scope).unwrap();
        let values = identities
            .iter()
            .map(|identity| {
                format!(
                    "{}/{}",
                    identity.record_id.as_str(),
                    identity.chunk_key.as_str()
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(values, ["a-record/m", "z-record/a", "z-record/z"]);
    }

    #[test]
    fn candidate_scope_operations_validate_generation_members_and_representations() {
        let mut corpus = CorpusIndex::new(CorpusId::new("scope-validation").unwrap());
        for index in 0..70 {
            corpus
                .upsert(record_input(
                    &format!("record-{index:02}"),
                    &[("body", BTreeMap::new())],
                ))
                .unwrap();
        }
        let dense = corpus.candidate_scope(0..70).unwrap();
        let sparse = corpus.candidate_scope([69]).unwrap();
        assert!(dense.is_dense());
        assert!(!sparse.is_dense());
        assert_eq!(corpus.filter_candidate_scope(&dense, None).unwrap(), dense);
        assert_eq!(corpus.candidate_scope_identities(&sparse).unwrap().len(), 1);

        let other = CorpusIndex::new(CorpusId::new("other").unwrap());
        assert!(matches!(
            other.filter_candidate_scope(&dense, None),
            Err(RetrievalKitError::StaleGeneration { .. })
        ));

        corpus.chunks[69].deleted = true;
        assert!(matches!(
            corpus.candidate_scope_identities(&sparse),
            Err(RetrievalKitError::InvalidCandidateScope { chunk_id: 69, .. })
        ));
        corpus.chunks[69].deleted = false;
        corpus.chunk_identities.remove(&69);
        assert!(matches!(
            corpus.candidate_scope_identities(&sparse),
            Err(RetrievalKitError::InvalidCandidateScope { chunk_id: 69, .. })
        ));

        corpus.generation = corpus.generation.next();
        assert!(matches!(
            corpus.filter_candidate_scope(&dense, Some(&Filter::exists("x"))),
            Err(RetrievalKitError::StaleGeneration { .. })
        ));
    }
}
