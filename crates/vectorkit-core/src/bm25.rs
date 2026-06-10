use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::error::{Result, VectorKitError};
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
    postings: HashMap<String, Vec<Posting>>,
    chunk_lengths: HashMap<ChunkId, usize>,
    active_chunks: HashSet<ChunkId>,
    active_total_length: usize,
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

    pub fn add_chunk(&mut self, chunk_id: ChunkId, text: &str, active: bool) {
        self.remove_chunk_terms(chunk_id);

        let terms = tokenize(text, &self.config.stop_words);
        let length = terms.len();
        let mut term_counts = BTreeMap::new();
        for term in terms {
            *term_counts.entry(term).or_insert(0) += 1;
        }

        for (term, count) in term_counts {
            self.postings.entry(term).or_default().push(Posting {
                chunk_id,
                term_frequency: count,
            });
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
        }
    }

    pub fn from_persisted(config: Bm25Config, persisted: PersistedBm25Index) -> Result<Self> {
        persisted.validate()?;
        let postings = persisted
            .postings
            .into_iter()
            .map(|(term, postings)| {
                (
                    term,
                    postings
                        .into_iter()
                        .map(|(chunk_id, term_frequency)| Posting {
                            chunk_id,
                            term_frequency,
                        })
                        .collect(),
                )
            })
            .collect();
        let chunk_lengths = persisted
            .chunk_lengths
            .into_iter()
            .collect::<HashMap<_, _>>();
        let active_chunks = persisted.active_chunks.into_iter().collect::<HashSet<_>>();
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
            .map(|(term, postings)| {
                let term_postings = postings
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
            let Some(postings) = self.postings.get(&term) else {
                continue;
            };

            let document_frequency = postings
                .iter()
                .filter(|posting| self.active_chunks.contains(&posting.chunk_id))
                .count();
            if document_frequency == 0 {
                continue;
            }

            let idf = inverse_document_frequency(active_count, document_frequency);
            for posting in postings {
                if !self.active_chunks.contains(&posting.chunk_id) {
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

        let mut hits = scores
            .into_iter()
            .map(|(chunk_id, score)| Bm25Hit {
                chunk_id,
                score,
                matched_terms: matched_terms.remove(&chunk_id).unwrap_or_default(),
            })
            .collect::<Vec<_>>();

        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.chunk_id.cmp(&right.chunk_id))
        });
        hits
    }

    pub fn estimated_payload_bytes(&self) -> usize {
        let term_bytes = self.postings.keys().map(String::len).sum::<usize>();
        let posting_bytes = self
            .postings
            .values()
            .map(|postings| {
                postings.len() * (std::mem::size_of::<ChunkId>() + std::mem::size_of::<usize>())
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
        if self.active_chunks.remove(&chunk_id) {
            if let Some(length) = self.chunk_lengths.get(&chunk_id) {
                self.active_total_length = self.active_total_length.saturating_sub(*length);
            }
        }
        self.chunk_lengths.remove(&chunk_id);

        let empty_terms = self
            .postings
            .iter_mut()
            .filter_map(|(term, postings)| {
                postings.retain(|posting| posting.chunk_id != chunk_id);
                postings.is_empty().then(|| term.clone())
            })
            .collect::<Vec<_>>();

        for term in empty_terms {
            self.postings.remove(&term);
        }
    }
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
            return Err(VectorKitError::InvalidFormat {
                message: "bm25 file has invalid magic".to_owned(),
            });
        }

        let format_version = reader.read_u32()?;
        if format_version != BM25_FORMAT_VERSION {
            return Err(VectorKitError::InvalidFormat {
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
                return Err(VectorKitError::InvalidFormat {
                    message: format!("bm25 active chunk {chunk_id} has no stored chunk length"),
                });
            }
        }

        for (term, postings) in &self.postings {
            if term.is_empty() {
                return Err(VectorKitError::InvalidFormat {
                    message: "bm25 term cannot be empty".to_owned(),
                });
            }

            for (chunk_id, term_frequency) in postings {
                if *term_frequency == 0 {
                    return Err(VectorKitError::InvalidFormat {
                        message: format!(
                            "bm25 term '{term}' has zero frequency for chunk {chunk_id}"
                        ),
                    });
                }

                if !self.chunk_lengths.contains_key(chunk_id) {
                    return Err(VectorKitError::InvalidFormat {
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
        String::from_utf8(bytes.to_vec()).map_err(|error| VectorKitError::InvalidFormat {
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
            .ok_or_else(|| VectorKitError::InvalidFormat {
                message: "bm25 reader offset overflow".to_owned(),
            })?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(VectorKitError::InvalidFormat {
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
            Err(VectorKitError::InvalidFormat {
                message: format!(
                    "bm25 file has {} trailing bytes",
                    self.bytes.len() - self.offset
                ),
            })
        }
    }
}

fn checked_usize_to_u32(value: usize, label: &str) -> Result<u32> {
    value.try_into().map_err(|_| VectorKitError::InvalidFormat {
        message: format!("{label} does not fit in u32"),
    })
}

fn checked_u32_to_usize(value: u32, label: &str) -> Result<usize> {
    value.try_into().map_err(|_| VectorKitError::InvalidFormat {
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
        assert!(restored.search_all("rust").is_empty());
    }

    #[test]
    fn persisted_bm25_state_rejects_bad_magic() {
        let error = PersistedBm25Index::from_payload_bytes(b"NOPE").unwrap_err();

        assert!(matches!(error, VectorKitError::InvalidFormat { .. }));
    }
}
