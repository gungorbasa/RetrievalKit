use std::collections::{BTreeMap, BTreeSet};

use crate::candidate_scope::CandidateScope;
use crate::error::{Result, VectorKitError};
use crate::record_store::{ChunkIdentity, CorpusId, GenerationId, Record, RecordId, RecordStore};
use crate::types::{ChunkId, StoredChunk};

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
                return Err(VectorKitError::InvalidCandidateScope {
                    chunk_id: *chunk_id,
                    message: "the ID is unavailable in this generation".to_owned(),
                });
            };
            if chunk.deleted {
                return Err(VectorKitError::InvalidCandidateScope {
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
                    VectorKitError::InvalidIdentity {
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

    pub(crate) fn validate_candidate_scope(&self, scope: &CandidateScope) -> Result<()> {
        if scope.corpus_id() == &self.corpus_id && scope.generation() == self.generation {
            return Ok(());
        }
        Err(VectorKitError::StaleGeneration {
            expected_corpus: self.corpus_id.as_str().to_owned(),
            expected_generation: self.generation.get(),
            actual_corpus: scope.corpus_id().as_str().to_owned(),
            actual_generation: scope.generation().get(),
        })
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
            return Err(VectorKitError::InvalidFormat {
                message: "canonical record corpus does not match the index corpus".to_owned(),
            });
        }
        self.record_store.validate()?;
        let mut seen_chunk_ids = BTreeSet::new();
        for (identity, chunk_id) in &self.chunk_ids_by_identity {
            if !seen_chunk_ids.insert(*chunk_id) {
                return Err(VectorKitError::InvalidFormat {
                    message: format!(
                        "multiple external chunk identities resolve to internal chunk ID {chunk_id}"
                    ),
                });
            }
            if self.record_store.get(&identity.record_id).is_none() {
                return Err(VectorKitError::InvalidFormat {
                    message: format!(
                        "chunk identity {}/{} references a missing canonical record",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    ),
                });
            }
            let Some(chunk) = self.chunk(*chunk_id) else {
                return Err(VectorKitError::InvalidFormat {
                    message: format!(
                        "chunk identity {}/{} references unavailable internal chunk ID {chunk_id}",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    ),
                });
            };
            if chunk.deleted || chunk.document_id != identity.record_id.as_str() {
                return Err(VectorKitError::InvalidFormat {
                    message: format!(
                        "chunk identity {}/{} does not resolve to its active canonical record",
                        identity.record_id.as_str(),
                        identity.chunk_key.as_str()
                    ),
                });
            }
            if self.chunk_identities.get(chunk_id) != Some(identity) {
                return Err(VectorKitError::InvalidFormat {
                    message: format!(
                        "reverse identity mapping is missing for internal chunk ID {chunk_id}"
                    ),
                });
            }
        }
        if self.chunk_identities.len() != self.chunk_ids_by_identity.len() {
            return Err(VectorKitError::InvalidFormat {
                message: "forward and reverse chunk identity mapping sizes differ".to_owned(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record_store::{ChunkKey, RecordType};

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
            Err(VectorKitError::StaleGeneration { .. })
        ));
    }
}
