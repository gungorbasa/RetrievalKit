use crate::record_store::{CorpusId, GenerationId};
use crate::types::ChunkId;

const BITS_PER_WORD: usize = u64::BITS as usize;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CandidateMembership {
    Sorted(Vec<ChunkId>),
    Dense { words: Vec<u64>, len: usize },
}

/// Validated, unranked chunk membership for one corpus generation.
///
/// Construction is owned by `CorpusIndex`, which rejects unavailable or
/// inactive IDs. The internal sparse/bitset choice is deliberately not public
/// ABI and may be tuned by benchmarks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateScope {
    corpus_id: CorpusId,
    generation: GenerationId,
    membership: CandidateMembership,
}

impl CandidateScope {
    pub(crate) fn from_sorted_ids(
        corpus_id: CorpusId,
        generation: GenerationId,
        ids: Vec<ChunkId>,
        universe: usize,
    ) -> Self {
        let dense_words = universe.div_ceil(BITS_PER_WORD);
        let sparse_bytes = ids.len().saturating_mul(std::mem::size_of::<ChunkId>());
        let dense_bytes = dense_words.saturating_mul(std::mem::size_of::<u64>());
        let membership = if !ids.is_empty() && dense_bytes <= sparse_bytes {
            let mut words = vec![0_u64; dense_words];
            for id in &ids {
                if let Ok(index) = usize::try_from(*id) {
                    words[index / BITS_PER_WORD] |= 1_u64 << (index % BITS_PER_WORD);
                }
            }
            CandidateMembership::Dense {
                words,
                len: ids.len(),
            }
        } else {
            CandidateMembership::Sorted(ids)
        };
        Self {
            corpus_id,
            generation,
            membership,
        }
    }

    pub fn corpus_id(&self) -> &CorpusId {
        &self.corpus_id
    }

    pub fn generation(&self) -> GenerationId {
        self.generation
    }

    pub fn len(&self) -> usize {
        match &self.membership {
            CandidateMembership::Sorted(ids) => ids.len(),
            CandidateMembership::Dense { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn is_dense(&self) -> bool {
        matches!(self.membership, CandidateMembership::Dense { .. })
    }

    pub(crate) fn contains(&self, chunk_id: ChunkId) -> bool {
        match &self.membership {
            CandidateMembership::Sorted(ids) => ids.binary_search(&chunk_id).is_ok(),
            CandidateMembership::Dense { words, .. } => usize::try_from(chunk_id)
                .ok()
                .and_then(|index| {
                    words
                        .get(index / BITS_PER_WORD)
                        .map(|word| word & (1_u64 << (index % BITS_PER_WORD)) != 0)
                })
                .unwrap_or(false),
        }
    }

    pub(crate) fn ids(&self) -> CandidateIds<'_> {
        match &self.membership {
            CandidateMembership::Sorted(ids) => CandidateIds::Sorted(ids.iter()),
            CandidateMembership::Dense { words, .. } => CandidateIds::Dense { words, next: 0 },
        }
    }
}

pub(crate) enum CandidateIds<'a> {
    Sorted(std::slice::Iter<'a, ChunkId>),
    Dense { words: &'a [u64], next: usize },
}

impl Iterator for CandidateIds<'_> {
    type Item = ChunkId;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Sorted(ids) => ids.next().copied(),
            Self::Dense { words, next } => {
                while *next < words.len().saturating_mul(BITS_PER_WORD) {
                    let index = *next;
                    *next += 1;
                    if words[index / BITS_PER_WORD] & (1_u64 << (index % BITS_PER_WORD)) != 0 {
                        return u64::try_from(index).ok();
                    }
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(ids: Vec<ChunkId>, universe: usize) -> CandidateScope {
        CandidateScope::from_sorted_ids(
            CorpusId::new("corpus").unwrap(),
            GenerationId::new(3),
            ids,
            universe,
        )
    }

    #[test]
    fn sparse_and_dense_representations_have_identical_membership_and_iteration() {
        let sparse = scope(vec![1, 7, 130], 10_000);
        let dense = scope((0..130).collect(), 130);
        assert!(!sparse.is_dense());
        assert!(dense.is_dense());
        assert_eq!(sparse.ids().collect::<Vec<_>>(), vec![1, 7, 130]);
        assert_eq!(
            dense.ids().collect::<Vec<_>>(),
            (0..130).collect::<Vec<_>>()
        );
        assert!(sparse.contains(7));
        assert!(!sparse.contains(8));
    }
}
