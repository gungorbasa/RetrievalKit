use std::error::Error;
use std::fmt::{Display, Formatter};

/// Boundary selection used when splitting text into chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkingStrategy {
    /// Split at the configured Unicode-character limit.
    Fixed,
    /// Prefer sentence endings, then whitespace, before the configured limit.
    Sentence,
}

/// Validated configuration for deterministic text chunking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkingConfig {
    pub max_characters: usize,
    pub overlap_characters: usize,
    pub strategy: ChunkingStrategy,
}

impl ChunkingConfig {
    pub fn fixed(max_characters: usize, overlap_characters: usize) -> Result<Self, ChunkingError> {
        Self::new(max_characters, overlap_characters, ChunkingStrategy::Fixed)
    }

    pub fn sentence(
        max_characters: usize,
        overlap_characters: usize,
    ) -> Result<Self, ChunkingError> {
        Self::new(
            max_characters,
            overlap_characters,
            ChunkingStrategy::Sentence,
        )
    }

    pub fn new(
        max_characters: usize,
        overlap_characters: usize,
        strategy: ChunkingStrategy,
    ) -> Result<Self, ChunkingError> {
        if max_characters == 0 {
            return Err(ChunkingError::ZeroMaximum);
        }
        if overlap_characters >= max_characters {
            return Err(ChunkingError::OverlapNotSmaller {
                overlap_characters,
                max_characters,
            });
        }
        Ok(Self {
            max_characters,
            overlap_characters,
            strategy,
        })
    }
}

/// A borrowed range copied from the input text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChunk {
    pub text: String,
    /// Inclusive UTF-8 byte offset in the original text.
    pub start_byte: usize,
    /// Exclusive UTF-8 byte offset in the original text.
    pub end_byte: usize,
}

/// Invalid chunking configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkingError {
    ZeroMaximum,
    OverlapNotSmaller {
        overlap_characters: usize,
        max_characters: usize,
    },
}

impl Display for ChunkingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMaximum => write!(f, "max_characters must be greater than zero"),
            Self::OverlapNotSmaller {
                overlap_characters,
                max_characters,
            } => write!(
                f,
                "overlap_characters ({overlap_characters}) must be smaller than max_characters ({max_characters})"
            ),
        }
    }
}

impl Error for ChunkingError {}

/// Splits UTF-8 text deterministically without cutting a Unicode scalar value.
pub fn chunk_text(text: &str, config: ChunkingConfig) -> Vec<TextChunk> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let mut boundaries: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
    boundaries.push(text.len());

    let character_count = boundaries.len() - 1;
    let mut chunks = Vec::new();
    let mut start_character = 0;

    while start_character < character_count {
        let hard_end = (start_character + config.max_characters).min(character_count);
        let end_character = match config.strategy {
            ChunkingStrategy::Fixed => hard_end,
            ChunkingStrategy::Sentence => preferred_boundary(
                text,
                &boundaries,
                start_character,
                hard_end,
                config.overlap_characters,
            ),
        };

        let raw_start = boundaries[start_character];
        let raw_end = boundaries[end_character];
        let (start_byte, end_byte) = trim_byte_range(text, raw_start, raw_end);
        if start_byte < end_byte {
            chunks.push(TextChunk {
                text: text[start_byte..end_byte].to_owned(),
                start_byte,
                end_byte,
            });
        }

        if end_character == character_count {
            break;
        }
        start_character = end_character
            .saturating_sub(config.overlap_characters)
            .max(start_character + 1);
    }

    chunks
}

fn preferred_boundary(
    text: &str,
    boundaries: &[usize],
    start_character: usize,
    hard_end: usize,
    overlap_characters: usize,
) -> usize {
    if hard_end == boundaries.len() - 1 {
        return hard_end;
    }

    let characters: Vec<char> = text[boundaries[start_character]..boundaries[hard_end]]
        .chars()
        .collect();
    let mut sentence_boundary = None;
    let mut whitespace_boundary = None;

    for (relative_index, character) in characters.iter().copied().enumerate() {
        let boundary = start_character + relative_index + 1;
        if boundary <= start_character + overlap_characters {
            continue;
        }
        if character.is_whitespace() {
            whitespace_boundary = Some(boundary);
        }
        if is_sentence_terminator(character)
            && characters
                .get(relative_index + 1)
                .is_none_or(|next| next.is_whitespace())
        {
            sentence_boundary = Some(boundary);
        }
    }

    sentence_boundary
        .or(whitespace_boundary)
        .unwrap_or(hard_end)
}

fn is_sentence_terminator(character: char) -> bool {
    matches!(
        character,
        '.' | '!' | '?' | '\u{3002}' | '\u{ff01}' | '\u{ff1f}'
    )
}

fn trim_byte_range(text: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    while start < end {
        let character = text[start..end]
            .chars()
            .next()
            .expect("non-empty UTF-8 range");
        if !character.is_whitespace() {
            break;
        }
        start += character.len_utf8();
    }
    while start < end {
        let character = text[start..end]
            .chars()
            .next_back()
            .expect("non-empty UTF-8 range");
        if !character.is_whitespace() {
            break;
        }
        end -= character.len_utf8();
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_chunks_preserve_unicode_offsets_and_overlap() {
        let text = "abçdef";
        let chunks = chunk_text(text, ChunkingConfig::fixed(4, 1).unwrap());

        assert_eq!(
            chunks,
            vec![
                TextChunk {
                    text: "abçd".to_owned(),
                    start_byte: 0,
                    end_byte: 5,
                },
                TextChunk {
                    text: "def".to_owned(),
                    start_byte: 4,
                    end_byte: 7,
                },
            ]
        );
    }

    #[test]
    fn sentence_chunks_prefer_sentence_endings() {
        let text = "First sentence. Second sentence. Third.";
        let chunks = chunk_text(text, ChunkingConfig::sentence(25, 0).unwrap());

        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<Vec<_>>(),
            vec!["First sentence.", "Second sentence. Third."]
        );
    }

    #[test]
    fn sentence_chunks_fall_back_to_whitespace_then_hard_limit() {
        let chunks = chunk_text(
            "alpha beta supercalifragilistic",
            ChunkingConfig::sentence(10, 0).unwrap(),
        );

        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta", "supercalif", "ragilistic"]
        );
    }

    #[test]
    fn blank_text_returns_no_chunks() {
        assert!(chunk_text(" \n\t", ChunkingConfig::fixed(10, 0).unwrap()).is_empty());
    }

    #[test]
    fn config_rejects_non_progressing_values() {
        assert_eq!(ChunkingConfig::fixed(0, 0), Err(ChunkingError::ZeroMaximum));
        assert_eq!(
            ChunkingConfig::fixed(5, 5),
            Err(ChunkingError::OverlapNotSmaller {
                overlap_characters: 5,
                max_characters: 5,
            })
        );
    }

    #[test]
    fn short_sentence_boundary_with_large_overlap_still_progresses() {
        let chunks = chunk_text(
            "Hi. This sentence is longer.",
            ChunkingConfig::sentence(10, 8).unwrap(),
        );

        assert!(!chunks.is_empty());
        assert_eq!(chunks.last().unwrap().end_byte, 28);
        assert!(chunks.len() <= "Hi. This sentence is longer.".chars().count());
    }
}
