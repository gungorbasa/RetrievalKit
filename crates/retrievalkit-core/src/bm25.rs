// Binary BM25 snapshot helpers are native-persistence implementation details.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::candidate_scope::CandidateScope;
use crate::error::{Result, RetrievalKitError};
use crate::types::ChunkId;

const BM25_MAGIC: &[u8; 4] = b"VKBM";
const BM25_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct Bm25Config {
    /// Term frequency saturation parameter. V1 default is `1.2`.
    pub k1: f32,
    /// Length normalization parameter. V1 default is `0.75`.
    pub b: f32,
    /// Lowercased terms ignored during indexing and query tokenization.
    pub stop_words: BTreeSet<String>,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self {
            k1: 1.2,
            b: 0.75,
            stop_words: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Bm25Hit {
    pub chunk_id: ChunkId,
    pub score: f32,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Bm25Index {
    config: Bm25Config,
    postings: HashMap<String, TermPostings>,
    chunk_lengths: HashMap<ChunkId, usize>,
    active_chunks: HashSet<ChunkId>,
    active_total_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TermPostings {
    postings: Vec<Posting>,
    active_document_frequency: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Posting {
    chunk_id: ChunkId,
    term_frequency: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PersistedBm25Index {
    postings: BTreeMap<String, BTreeMap<ChunkId, usize>>,
    chunk_lengths: BTreeMap<ChunkId, usize>,
    active_chunks: BTreeSet<ChunkId>,
}

impl Bm25Index {
    pub fn new(config: Bm25Config) -> Self {
        Self {
            config,
            postings: HashMap::new(),
            chunk_lengths: HashMap::new(),
            active_chunks: HashSet::new(),
            active_total_length: 0,
        }
    }

    pub fn config(&self) -> &Bm25Config {
        &self.config
    }

    pub fn add_chunk(&mut self, chunk_id: ChunkId, text: &str, active: bool) {
        self.remove_chunk_terms(chunk_id);

        let terms = tokenize(text, &self.config.stop_words);
        let length = terms.len();
        let mut term_counts = BTreeMap::new();
        for term in terms {
            *term_counts.entry(term).or_insert(0) += 1;
        }

        for (term, count) in term_counts {
            let term_postings = self.postings.entry(term).or_default();
            term_postings.postings.push(Posting {
                chunk_id,
                term_frequency: count,
            });
            if active {
                term_postings.active_document_frequency += 1;
            }
        }

        self.chunk_lengths.insert(chunk_id, length);
        if active && self.active_chunks.insert(chunk_id) {
            self.active_total_length += length;
        }
    }

    pub fn deactivate_chunk(&mut self, chunk_id: ChunkId) {
        if self.active_chunks.remove(&chunk_id) {
            if let Some(length) = self.chunk_lengths.get(&chunk_id) {
                self.active_total_length = self.active_total_length.saturating_sub(*length);
            }
            self.decrement_active_document_frequencies(chunk_id);
        }
    }

    pub fn from_persisted(config: Bm25Config, persisted: PersistedBm25Index) -> Result<Self> {
        persisted.validate()?;
        let persisted_active_chunks = persisted.active_chunks;
        let postings = persisted
            .postings
            .into_iter()
            .map(|(term, postings)| {
                let mut active_document_frequency = 0;
                let postings = postings
                    .into_iter()
                    .map(|(chunk_id, term_frequency)| {
                        if persisted_active_chunks.contains(&chunk_id) {
                            active_document_frequency += 1;
                        }
                        Posting {
                            chunk_id,
                            term_frequency,
                        }
                    })
                    .collect();
                (
                    term,
                    TermPostings {
                        postings,
                        active_document_frequency,
                    },
                )
            })
            .collect();
        let chunk_lengths = persisted
            .chunk_lengths
            .into_iter()
            .collect::<HashMap<_, _>>();
        let active_chunks = persisted_active_chunks.into_iter().collect::<HashSet<_>>();
        let active_total_length = active_chunks
            .iter()
            .filter_map(|chunk_id| chunk_lengths.get(chunk_id))
            .sum();

        Ok(Self {
            config,
            postings,
            chunk_lengths,
            active_chunks,
            active_total_length,
        })
    }

    pub fn to_persisted(&self) -> PersistedBm25Index {
        let postings = self
            .postings
            .iter()
            .map(|(term, term_postings)| {
                let term_postings = term_postings
                    .postings
                    .iter()
                    .map(|posting| (posting.chunk_id, posting.term_frequency))
                    .collect::<BTreeMap<_, _>>();
                (term.clone(), term_postings)
            })
            .collect::<BTreeMap<_, _>>();
        let chunk_lengths = self
            .chunk_lengths
            .iter()
            .map(|(chunk_id, length)| (*chunk_id, *length))
            .collect::<BTreeMap<_, _>>();
        let active_chunks = self.active_chunks.iter().copied().collect::<BTreeSet<_>>();

        PersistedBm25Index {
            postings,
            chunk_lengths,
            active_chunks,
        }
    }

    pub fn search_all(&self, query: &str) -> Vec<Bm25Hit> {
        self.search_with_limit(query, None, None)
    }

    pub fn search_top_k(&self, query: &str, top_k: usize) -> Vec<Bm25Hit> {
        if top_k == 0 {
            return Vec::new();
        }

        self.search_with_limit(query, Some(top_k), None)
    }

    pub fn search_top_k_in_chunks(
        &self,
        query: &str,
        top_k: usize,
        allowed_chunks: &HashSet<ChunkId>,
    ) -> Vec<Bm25Hit> {
        if top_k == 0 || allowed_chunks.is_empty() {
            return Vec::new();
        }

        self.search_with_limit(query, Some(top_k), Some(allowed_chunks))
    }

    pub fn search_top_k_in_scope(
        &self,
        query: &str,
        top_k: usize,
        scope: &CandidateScope,
    ) -> Vec<Bm25Hit> {
        if top_k == 0 || scope.is_empty() {
            return Vec::new();
        }

        let query_terms = tokenize(query, &self.config.stop_words)
            .into_iter()
            .collect::<BTreeSet<_>>();
        if query_terms.is_empty() || self.active_chunks.is_empty() {
            return Vec::new();
        }
        let active_count = self.active_chunks.len();
        let average_length = self.average_active_chunk_length();
        if average_length == 0.0 {
            return Vec::new();
        }

        let mut scores: HashMap<ChunkId, f32> = HashMap::new();
        let mut matched_terms: HashMap<ChunkId, Vec<String>> = HashMap::new();
        for term in query_terms {
            let Some(term_postings) = self.postings.get(&term) else {
                continue;
            };
            let document_frequency = term_postings.active_document_frequency;
            if document_frequency == 0 {
                continue;
            }
            let idf = inverse_document_frequency(active_count, document_frequency);
            for posting in &term_postings.postings {
                if !self.active_chunks.contains(&posting.chunk_id)
                    || !scope.contains(posting.chunk_id)
                {
                    continue;
                }
                let Some(&chunk_length) = self.chunk_lengths.get(&posting.chunk_id) else {
                    continue;
                };
                let score = bm25_term_score(
                    posting.term_frequency,
                    chunk_length,
                    average_length,
                    idf,
                    self.config.k1,
                    self.config.b,
                );
                *scores.entry(posting.chunk_id).or_insert(0.0) += score;
                matched_terms
                    .entry(posting.chunk_id)
                    .or_default()
                    .push(term.clone());
            }
        }

        let mut bounded_hits = Bm25HitTopK::new(top_k);
        for (chunk_id, score) in scores {
            bounded_hits.push(Bm25Hit {
                chunk_id,
                score,
                matched_terms: matched_terms.remove(&chunk_id).unwrap_or_default(),
            });
        }
        bounded_hits.into_sorted_vec()
    }

    fn search_with_limit(
        &self,
        query: &str,
        limit: Option<usize>,
        allowed_chunks: Option<&HashSet<ChunkId>>,
    ) -> Vec<Bm25Hit> {
        let query_terms = tokenize(query, &self.config.stop_words)
            .into_iter()
            .collect::<BTreeSet<_>>();

        if query_terms.is_empty()
            || self.active_chunks.is_empty()
            || allowed_chunks.is_some_and(|allowed_chunks| allowed_chunks.is_empty())
        {
            return Vec::new();
        }

        let active_count = self.active_chunks.len();
        let average_length = self.average_active_chunk_length();
        if average_length == 0.0 {
            return Vec::new();
        }

        let mut scores: HashMap<ChunkId, f32> = HashMap::new();
        let mut matched_terms: HashMap<ChunkId, Vec<String>> = HashMap::new();

        for term in query_terms {
            let Some(term_postings) = self.postings.get(&term) else {
                continue;
            };

            let document_frequency = term_postings.active_document_frequency;
            if document_frequency == 0 {
                continue;
            }

            let idf = inverse_document_frequency(active_count, document_frequency);
            for posting in &term_postings.postings {
                if !self.active_chunks.contains(&posting.chunk_id) {
                    continue;
                }
                if let Some(allowed_chunks) = allowed_chunks {
                    if !allowed_chunks.contains(&posting.chunk_id) {
                        continue;
                    }
                }

                let Some(&chunk_length) = self.chunk_lengths.get(&posting.chunk_id) else {
                    continue;
                };

                let score = bm25_term_score(
                    posting.term_frequency,
                    chunk_length,
                    average_length,
                    idf,
                    self.config.k1,
                    self.config.b,
                );
                *scores.entry(posting.chunk_id).or_insert(0.0) += score;
                matched_terms
                    .entry(posting.chunk_id)
                    .or_default()
                    .push(term.clone());
            }
        }

        if let Some(limit) = limit {
            let mut bounded_hits = Bm25HitTopK::new(limit);
            for (chunk_id, score) in scores {
                let hit = Bm25Hit {
                    chunk_id,
                    score,
                    matched_terms: matched_terms.remove(&chunk_id).unwrap_or_default(),
                };
                bounded_hits.push(hit);
            }
            return bounded_hits.into_sorted_vec();
        }

        let mut hits = scores
            .into_iter()
            .map(|(chunk_id, score)| Bm25Hit {
                chunk_id,
                score,
                matched_terms: matched_terms.remove(&chunk_id).unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        sort_bm25_hits(&mut hits);
        hits
    }

    pub fn estimated_payload_bytes(&self) -> usize {
        let term_bytes = self.postings.keys().map(String::len).sum::<usize>();
        let posting_bytes = self
            .postings
            .values()
            .map(|term_postings| {
                term_postings.postings.len()
                    * (std::mem::size_of::<ChunkId>() + std::mem::size_of::<usize>())
                    + std::mem::size_of::<usize>()
            })
            .sum::<usize>();
        let chunk_length_bytes = self.chunk_lengths.len()
            * (std::mem::size_of::<ChunkId>() + std::mem::size_of::<usize>());
        let active_chunk_bytes = self.active_chunks.len() * std::mem::size_of::<ChunkId>();

        term_bytes + posting_bytes + chunk_length_bytes + active_chunk_bytes
    }

    fn average_active_chunk_length(&self) -> f32 {
        self.active_total_length as f32 / self.active_chunks.len() as f32
    }

    fn remove_chunk_terms(&mut self, chunk_id: ChunkId) {
        let was_active = self.active_chunks.remove(&chunk_id);
        if was_active {
            if let Some(length) = self.chunk_lengths.get(&chunk_id) {
                self.active_total_length = self.active_total_length.saturating_sub(*length);
            }
            self.decrement_active_document_frequencies(chunk_id);
        }
        self.chunk_lengths.remove(&chunk_id);

        let empty_terms = self
            .postings
            .iter_mut()
            .filter_map(|(term, term_postings)| {
                term_postings
                    .postings
                    .retain(|posting| posting.chunk_id != chunk_id);
                term_postings.postings.is_empty().then(|| term.clone())
            })
            .collect::<Vec<_>>();

        for term in empty_terms {
            self.postings.remove(&term);
        }
    }

    fn decrement_active_document_frequencies(&mut self, chunk_id: ChunkId) {
        for term_postings in self.postings.values_mut() {
            if term_postings
                .postings
                .iter()
                .any(|posting| posting.chunk_id == chunk_id)
            {
                term_postings.active_document_frequency =
                    term_postings.active_document_frequency.saturating_sub(1);
            }
        }
    }
}

struct Bm25HitTopK {
    top_k: usize,
    heap: BinaryHeap<HeapBm25Hit>,
}

impl Bm25HitTopK {
    fn new(top_k: usize) -> Self {
        Self {
            top_k,
            heap: BinaryHeap::with_capacity(top_k),
        }
    }

    fn push(&mut self, hit: Bm25Hit) {
        if self.heap.len() < self.top_k {
            self.heap.push(HeapBm25Hit(hit));
            return;
        }

        let Some(worst) = self.heap.peek() else {
            return;
        };

        if bm25_hit_ranks_before(&hit, &worst.0) {
            self.heap.pop();
            self.heap.push(HeapBm25Hit(hit));
        }
    }

    fn into_sorted_vec(self) -> Vec<Bm25Hit> {
        let mut hits = self.heap.into_iter().map(|hit| hit.0).collect::<Vec<_>>();
        sort_bm25_hits(&mut hits);
        hits
    }
}

#[derive(Debug, Clone, PartialEq)]
struct HeapBm25Hit(Bm25Hit);

impl Eq for HeapBm25Hit {}

impl Ord for HeapBm25Hit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .score
            .total_cmp(&self.0.score)
            .then_with(|| self.0.chunk_id.cmp(&other.0.chunk_id))
    }
}

impl PartialOrd for HeapBm25Hit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn sort_bm25_hits(hits: &mut [Bm25Hit]) {
    hits.sort_by(compare_bm25_hits);
}

fn compare_bm25_hits(left: &Bm25Hit, right: &Bm25Hit) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn bm25_hit_ranks_before(left: &Bm25Hit, right: &Bm25Hit) -> bool {
    compare_bm25_hits(left, right).is_lt()
}

impl PersistedBm25Index {
    pub fn to_payload_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(BM25_MAGIC);
        write_u32(&mut bytes, BM25_FORMAT_VERSION);
        write_u32(
            &mut bytes,
            checked_usize_to_u32(self.postings.len(), "bm25 term count")?,
        );
        for (term, postings) in &self.postings {
            write_string(&mut bytes, term)?;
            write_u32(
                &mut bytes,
                checked_usize_to_u32(postings.len(), "bm25 posting count")?,
            );
            for (chunk_id, term_frequency) in postings {
                write_u64(&mut bytes, *chunk_id);
                write_u32(
                    &mut bytes,
                    checked_usize_to_u32(*term_frequency, "bm25 term frequency")?,
                );
            }
        }

        write_u32(
            &mut bytes,
            checked_usize_to_u32(self.chunk_lengths.len(), "bm25 chunk length count")?,
        );
        for (chunk_id, length) in &self.chunk_lengths {
            write_u64(&mut bytes, *chunk_id);
            write_u32(
                &mut bytes,
                checked_usize_to_u32(*length, "bm25 chunk length")?,
            );
        }

        write_u32(
            &mut bytes,
            checked_usize_to_u32(self.active_chunks.len(), "bm25 active chunk count")?,
        );
        for chunk_id in &self.active_chunks {
            write_u64(&mut bytes, *chunk_id);
        }

        Ok(bytes)
    }

    pub fn from_payload_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Bm25ByteReader::new(bytes);
        if reader.read_exact(BM25_MAGIC.len())? != BM25_MAGIC {
            return Err(RetrievalKitError::InvalidFormat {
                message: "bm25 file has invalid magic".to_owned(),
            });
        }

        let format_version = reader.read_u32()?;
        if format_version != BM25_FORMAT_VERSION {
            return Err(RetrievalKitError::InvalidFormat {
                message: format!("unsupported bm25 file version {format_version}"),
            });
        }

        let term_count = checked_u32_to_usize(reader.read_u32()?, "bm25 term count")?;
        let mut postings = BTreeMap::new();
        for _ in 0..term_count {
            let term = reader.read_string()?;
            let posting_count = checked_u32_to_usize(reader.read_u32()?, "bm25 posting count")?;
            let mut term_postings = BTreeMap::new();
            for _ in 0..posting_count {
                term_postings.insert(
                    reader.read_u64()?,
                    checked_u32_to_usize(reader.read_u32()?, "bm25 term frequency")?,
                );
            }
            postings.insert(term, term_postings);
        }

        let chunk_length_count =
            checked_u32_to_usize(reader.read_u32()?, "bm25 chunk length count")?;
        let mut chunk_lengths = BTreeMap::new();
        for _ in 0..chunk_length_count {
            chunk_lengths.insert(
                reader.read_u64()?,
                checked_u32_to_usize(reader.read_u32()?, "bm25 chunk length")?,
            );
        }

        let active_chunk_count =
            checked_u32_to_usize(reader.read_u32()?, "bm25 active chunk count")?;
        let mut active_chunks = BTreeSet::new();
        for _ in 0..active_chunk_count {
            active_chunks.insert(reader.read_u64()?);
        }

        reader.finish()?;
        let persisted = Self {
            postings,
            chunk_lengths,
            active_chunks,
        };
        persisted.validate()?;
        Ok(persisted)
    }

    pub fn active_chunk_ids(&self) -> impl Iterator<Item = &ChunkId> {
        self.active_chunks.iter()
    }

    pub fn chunk_length_ids(&self) -> impl Iterator<Item = &ChunkId> {
        self.chunk_lengths.keys()
    }

    fn validate(&self) -> Result<()> {
        for chunk_id in &self.active_chunks {
            if !self.chunk_lengths.contains_key(chunk_id) {
                return Err(RetrievalKitError::InvalidFormat {
                    message: format!("bm25 active chunk {chunk_id} has no stored chunk length"),
                });
            }
        }

        for (term, postings) in &self.postings {
            if term.is_empty() {
                return Err(RetrievalKitError::InvalidFormat {
                    message: "bm25 term cannot be empty".to_owned(),
                });
            }

            for (chunk_id, term_frequency) in postings {
                if *term_frequency == 0 {
                    return Err(RetrievalKitError::InvalidFormat {
                        message: format!(
                            "bm25 term '{term}' has zero frequency for chunk {chunk_id}"
                        ),
                    });
                }

                if !self.chunk_lengths.contains_key(chunk_id) {
                    return Err(RetrievalKitError::InvalidFormat {
                        message: format!("bm25 term '{term}' references missing chunk {chunk_id}"),
                    });
                }
            }
        }

        Ok(())
    }
}

fn write_string(bytes: &mut Vec<u8>, value: &str) -> Result<()> {
    write_u32(
        bytes,
        checked_usize_to_u32(value.len(), "bm25 string length")?,
    );
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

struct Bm25ByteReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Bm25ByteReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_string(&mut self) -> Result<String> {
        let len = checked_u32_to_usize(self.read_u32()?, "bm25 string length")?;
        let bytes = self.read_exact(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|error| RetrievalKitError::InvalidFormat {
            message: format!("invalid UTF-8 string in bm25 file: {error}"),
        })
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

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| RetrievalKitError::InvalidFormat {
                message: "bm25 reader offset overflow".to_owned(),
            })?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(RetrievalKitError::InvalidFormat {
                message: "bm25 file ended unexpectedly".to_owned(),
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
                    "bm25 file has {} trailing bytes",
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

fn checked_u32_to_usize(value: u32, label: &str) -> Result<usize> {
    value
        .try_into()
        .map_err(|_| RetrievalKitError::InvalidFormat {
            message: format!("{label} does not fit in usize"),
        })
}

fn bm25_term_score(
    term_frequency: usize,
    chunk_length: usize,
    average_length: f32,
    idf: f32,
    k1: f32,
    b: f32,
) -> f32 {
    let term_frequency = term_frequency as f32;
    let length_ratio = chunk_length as f32 / average_length;
    let denominator = term_frequency + k1 * (1.0 - b + b * length_ratio);

    idf * (term_frequency * (k1 + 1.0)) / denominator
}

fn inverse_document_frequency(active_count: usize, document_frequency: usize) -> f32 {
    (1.0 + (active_count as f32 - document_frequency as f32 + 0.5)
        / (document_frequency as f32 + 0.5))
        .ln()
}

fn tokenize(text: &str, stop_words: &BTreeSet<String>) -> Vec<String> {
    text.unicode_words()
        .filter_map(|token| {
            let token = token.to_lowercase();
            (!token.is_empty() && !stop_words.contains(&token)).then_some(token)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_search_returns_matched_terms() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(1, "Swift local search", true);
        index.add_chunk(2, "Rust vector core", true);

        let hits = index.search_all("swift search");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_eq!(hits[0].matched_terms, vec!["search", "swift"]);
    }

    #[test]
    fn bm25_search_excludes_inactive_chunks() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(1, "deleted exact name", true);
        index.deactivate_chunk(1);

        let hits = index.search_all("exact name");

        assert!(hits.is_empty());
    }

    #[test]
    fn bm25_active_document_frequency_updates_when_chunks_deactivate() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(1, "target shared", true);
        index.add_chunk(2, "target", true);

        assert_eq!(
            index
                .postings
                .get("target")
                .map(|postings| postings.active_document_frequency),
            Some(2)
        );

        index.deactivate_chunk(1);

        assert_eq!(
            index
                .postings
                .get("target")
                .map(|postings| postings.active_document_frequency),
            Some(1)
        );
        assert_eq!(index.search_all("shared"), Vec::new());
    }

    #[test]
    fn bm25_active_document_frequency_updates_when_chunks_are_replaced() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(1, "old target", true);
        index.add_chunk(1, "new target", true);

        assert!(!index.postings.contains_key("old"));
        assert_eq!(
            index
                .postings
                .get("new")
                .map(|postings| postings.active_document_frequency),
            Some(1)
        );
        assert_eq!(
            index
                .postings
                .get("target")
                .map(|postings| postings.active_document_frequency),
            Some(1)
        );
    }

    #[test]
    fn bm25_search_is_deterministic_for_tied_scores() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(20, "same", true);
        index.add_chunk(10, "same", true);

        let hits = index.search_all("same");

        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    #[test]
    fn bm25_search_top_k_matches_search_all_prefix() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(1, "swift search search", true);
        index.add_chunk(2, "swift search", true);
        index.add_chunk(3, "swift", true);
        index.add_chunk(4, "search", true);

        let all_hits = index.search_all("swift search");
        let top_hits = index.search_top_k("swift search", 2);

        assert_eq!(top_hits, all_hits.into_iter().take(2).collect::<Vec<_>>());
    }

    #[test]
    fn bm25_search_top_k_keeps_deterministic_tie_ordering() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(20, "same", true);
        index.add_chunk(10, "same", true);

        let hits = index.search_top_k("same", 1);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 10);
    }

    #[test]
    fn bm25_search_top_k_in_chunks_scores_only_allowed_chunks() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(10, "target target target", true);
        index.add_chunk(20, "target", true);

        let allowed_chunks = [20].into_iter().collect::<HashSet<_>>();
        let hits = index.search_top_k_in_chunks("target", 1, &allowed_chunks);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 20);
    }

    #[test]
    fn bm25_configured_stop_words_are_ignored() {
        let mut stop_words = BTreeSet::new();
        stop_words.insert("the".to_owned());
        let mut index = Bm25Index::new(Bm25Config {
            stop_words,
            ..Bm25Config::default()
        });
        index.add_chunk(1, "the exact target", true);

        let hits = index.search_all("the");

        assert!(hits.is_empty());
    }

    #[test]
    fn bm25_tokenizer_keeps_unicode_words() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(1, "İstanbul'da hızlı arama", true);

        let hits = index.search_all("hızlı arama");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_eq!(hits[0].matched_terms, vec!["arama", "hızlı"]);
    }

    #[test]
    fn persisted_bm25_state_round_trips_keyword_results() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_chunk(1, "Swift local search", true);
        index.add_chunk(2, "Rust vector core", true);
        index.deactivate_chunk(2);

        let bytes = index.to_persisted().to_payload_bytes().unwrap();
        let persisted = PersistedBm25Index::from_payload_bytes(&bytes).unwrap();
        let restored = Bm25Index::from_persisted(Bm25Config::default(), persisted).unwrap();

        let hits = restored.search_all("swift search");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert_eq!(
            restored
                .postings
                .get("swift")
                .map(|postings| postings.active_document_frequency),
            Some(1)
        );
        assert_eq!(
            restored
                .postings
                .get("rust")
                .map(|postings| postings.active_document_frequency),
            Some(0)
        );
        assert!(restored.search_all("rust").is_empty());
    }

    #[test]
    fn persisted_bm25_state_rejects_bad_magic() {
        let error = PersistedBm25Index::from_payload_bytes(b"NOPE").unwrap_err();

        assert!(matches!(error, RetrievalKitError::InvalidFormat { .. }));
    }
}
