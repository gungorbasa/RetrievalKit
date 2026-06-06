use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::error::{Result, VectorKitError};
use crate::types::ChunkId;

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
    postings: BTreeMap<String, BTreeMap<ChunkId, usize>>,
    chunk_lengths: BTreeMap<ChunkId, usize>,
    active_chunks: BTreeSet<ChunkId>,
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
            postings: BTreeMap::new(),
            chunk_lengths: BTreeMap::new(),
            active_chunks: BTreeSet::new(),
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
            self.postings
                .entry(term)
                .or_default()
                .insert(chunk_id, count);
        }

        self.chunk_lengths.insert(chunk_id, length);
        if active {
            self.active_chunks.insert(chunk_id);
        } else {
            self.active_chunks.remove(&chunk_id);
        }
    }

    pub fn deactivate_chunk(&mut self, chunk_id: ChunkId) {
        self.active_chunks.remove(&chunk_id);
    }

    pub fn from_persisted(config: Bm25Config, persisted: PersistedBm25Index) -> Result<Self> {
        persisted.validate()?;
        Ok(Self {
            config,
            postings: persisted.postings,
            chunk_lengths: persisted.chunk_lengths,
            active_chunks: persisted.active_chunks,
        })
    }

    pub fn to_persisted(&self) -> PersistedBm25Index {
        PersistedBm25Index {
            postings: self.postings.clone(),
            chunk_lengths: self.chunk_lengths.clone(),
            active_chunks: self.active_chunks.clone(),
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

        let mut scores: BTreeMap<ChunkId, f32> = BTreeMap::new();
        let mut matched_terms: BTreeMap<ChunkId, Vec<String>> = BTreeMap::new();

        for term in query_terms {
            let Some(postings) = self.postings.get(&term) else {
                continue;
            };

            let document_frequency = postings
                .keys()
                .filter(|chunk_id| self.active_chunks.contains(chunk_id))
                .count();
            if document_frequency == 0 {
                continue;
            }

            let idf = inverse_document_frequency(active_count, document_frequency);
            for (&chunk_id, &term_frequency) in postings {
                if !self.active_chunks.contains(&chunk_id) {
                    continue;
                }

                let Some(&chunk_length) = self.chunk_lengths.get(&chunk_id) else {
                    continue;
                };

                let score = bm25_term_score(
                    term_frequency,
                    chunk_length,
                    average_length,
                    idf,
                    self.config.k1,
                    self.config.b,
                );
                *scores.entry(chunk_id).or_insert(0.0) += score;
                matched_terms
                    .entry(chunk_id)
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
        let total_length = self
            .active_chunks
            .iter()
            .filter_map(|chunk_id| self.chunk_lengths.get(chunk_id))
            .sum::<usize>();

        total_length as f32 / self.active_chunks.len() as f32
    }

    fn remove_chunk_terms(&mut self, chunk_id: ChunkId) {
        self.active_chunks.remove(&chunk_id);
        self.chunk_lengths.remove(&chunk_id);

        let empty_terms = self
            .postings
            .iter_mut()
            .filter_map(|(term, postings)| {
                postings.remove(&chunk_id);
                postings.is_empty().then(|| term.clone())
            })
            .collect::<Vec<_>>();

        for term in empty_terms {
            self.postings.remove(&term);
        }
    }
}

impl PersistedBm25Index {
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

        let restored =
            Bm25Index::from_persisted(Bm25Config::default(), index.to_persisted()).unwrap();

        let hits = restored.search_all("swift search");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, 1);
        assert!(restored.search_all("rust").is_empty());
    }
}
