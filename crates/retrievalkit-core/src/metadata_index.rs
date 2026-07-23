use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use crate::error::{Result, RetrievalKitError};
use crate::filter::Filter;
use crate::metadata::{Metadata, MetadataValue};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct MetadataFilterIndex {
    active_offsets: BTreeSet<usize>,
    fields: BTreeMap<String, FieldIndex>,
}

impl MetadataFilterIndex {
    pub fn insert(&mut self, offset: usize, metadata: &Metadata) {
        self.active_offsets.insert(offset);

        for (field, value) in metadata {
            let field_index = self.fields.entry(field.clone()).or_default();
            field_index.exists.insert(offset);

            if let Some(indexed_value) = IndexedValue::from_metadata(value) {
                field_index
                    .values
                    .entry(indexed_value)
                    .or_default()
                    .insert(offset);
            }

            if let Some(number) = IndexedNumber::from_metadata(value) {
                field_index
                    .numbers
                    .entry(number)
                    .or_default()
                    .insert(offset);
            }
        }
    }

    pub fn remove(&mut self, offset: usize, metadata: &Metadata) {
        self.active_offsets.remove(&offset);

        for (field, value) in metadata {
            let Some(field_index) = self.fields.get_mut(field) else {
                continue;
            };

            field_index.exists.remove(&offset);

            if let Some(indexed_value) = IndexedValue::from_metadata(value) {
                remove_offset(&mut field_index.values, &indexed_value, offset);
            }

            if let Some(number) = IndexedNumber::from_metadata(value) {
                remove_offset(&mut field_index.numbers, &number, offset);
            }
        }
    }

    pub fn candidate_offsets(&self, filter: &Filter) -> Result<Option<Vec<usize>>> {
        self.candidate_set(filter)
            .map(|candidate_set| candidate_set.map(|set| set.into_iter().collect()))
    }

    pub fn estimated_payload_bytes(&self) -> usize {
        let active_offset_bytes = self.active_offsets.len() * std::mem::size_of::<usize>();
        let field_bytes = self
            .fields
            .iter()
            .map(|(field, field_index)| field.len() + field_index.estimated_payload_bytes())
            .sum::<usize>();

        active_offset_bytes + field_bytes
    }

    fn candidate_set(&self, filter: &Filter) -> Result<Option<BTreeSet<usize>>> {
        match filter {
            Filter::Equals { field, value } => Ok(Some(self.equals_set(field, value))),
            Filter::NotEquals { field, value } => Ok(Some(self.not_equals_set(field, value))),
            Filter::In { field, values } => Ok(Some(self.in_set(field, values))),
            Filter::Range {
                field,
                lower,
                upper,
            } => self.range_set(field, lower.as_ref(), upper.as_ref()),
            Filter::Exists { field } => Ok(Some(self.exists_set(field))),
            Filter::All(filters) => self.all_set(filters),
            Filter::Any(filters) => self.any_set(filters),
        }
    }

    fn equals_set(&self, field: &str, value: &MetadataValue) -> BTreeSet<usize> {
        let Some(indexed_value) = IndexedValue::from_metadata(value) else {
            return BTreeSet::new();
        };

        self.fields
            .get(field)
            .and_then(|field_index| field_index.values.get(&indexed_value))
            .cloned()
            .unwrap_or_default()
    }

    fn not_equals_set(&self, field: &str, value: &MetadataValue) -> BTreeSet<usize> {
        let matching = self.equals_set(field, value);
        self.active_offsets.difference(&matching).copied().collect()
    }

    fn in_set(&self, field: &str, values: &[MetadataValue]) -> BTreeSet<usize> {
        let mut candidates = BTreeSet::new();
        for value in values {
            candidates.extend(self.equals_set(field, value));
        }
        candidates
    }

    fn range_set(
        &self,
        field: &str,
        lower: Option<&MetadataValue>,
        upper: Option<&MetadataValue>,
    ) -> Result<Option<BTreeSet<usize>>> {
        let lower = range_bound(field, lower)?;
        let upper = range_bound(field, upper)?;

        if lower == RangeIndexBound::NonFinite || upper == RangeIndexBound::NonFinite {
            return Ok(None);
        }

        if lower.is_greater_than(upper) {
            return Ok(Some(BTreeSet::new()));
        }

        let Some(field_index) = self.fields.get(field) else {
            return Ok(Some(BTreeSet::new()));
        };

        let mut candidates = BTreeSet::new();
        for (_, offsets) in field_index
            .numbers
            .range((lower.as_lower_bound(), upper.as_upper_bound()))
        {
            candidates.extend(offsets);
        }
        Ok(Some(candidates))
    }

    fn exists_set(&self, field: &str) -> BTreeSet<usize> {
        self.fields
            .get(field)
            .map(|field_index| field_index.exists.clone())
            .unwrap_or_default()
    }

    fn all_set(&self, filters: &[Filter]) -> Result<Option<BTreeSet<usize>>> {
        let mut candidates = None::<BTreeSet<usize>>;

        for filter in filters {
            let Some(child_candidates) = self.candidate_set(filter)? else {
                continue;
            };

            candidates = Some(match candidates {
                Some(existing) => existing
                    .intersection(&child_candidates)
                    .copied()
                    .collect::<BTreeSet<_>>(),
                None => child_candidates,
            });
        }

        Ok(Some(
            candidates.unwrap_or_else(|| self.active_offsets.clone()),
        ))
    }

    fn any_set(&self, filters: &[Filter]) -> Result<Option<BTreeSet<usize>>> {
        let mut candidates = BTreeSet::new();

        for filter in filters {
            let Some(child_candidates) = self.candidate_set(filter)? else {
                return Ok(None);
            };
            candidates.extend(child_candidates);
        }

        Ok(Some(candidates))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct FieldIndex {
    exists: BTreeSet<usize>,
    values: BTreeMap<IndexedValue, BTreeSet<usize>>,
    numbers: BTreeMap<IndexedNumber, BTreeSet<usize>>,
}

impl FieldIndex {
    fn estimated_payload_bytes(&self) -> usize {
        let exists_bytes = self.exists.len() * std::mem::size_of::<usize>();
        let value_bytes = self
            .values
            .iter()
            .map(|(value, offsets)| {
                value.estimated_payload_bytes() + offsets.len() * std::mem::size_of::<usize>()
            })
            .sum::<usize>();
        let number_bytes = self
            .numbers
            .values()
            .map(|offsets| {
                std::mem::size_of::<f64>() + offsets.len() * std::mem::size_of::<usize>()
            })
            .sum::<usize>();

        exists_bytes + value_bytes + number_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum IndexedValue {
    String(String),
    Integer(i64),
    Float(IndexedNumber),
    Boolean(bool),
    TimestampMillis(i64),
}

impl IndexedValue {
    fn from_metadata(value: &MetadataValue) -> Option<Self> {
        match value {
            MetadataValue::String(value) => Some(Self::String(value.clone())),
            MetadataValue::Integer(value) => Some(Self::Integer(*value)),
            MetadataValue::Float(value) => IndexedNumber::new(*value).map(Self::Float),
            MetadataValue::Boolean(value) => Some(Self::Boolean(*value)),
            MetadataValue::TimestampMillis(value) => Some(Self::TimestampMillis(*value)),
        }
    }

    fn estimated_payload_bytes(&self) -> usize {
        match self {
            Self::String(value) => value.len(),
            Self::Integer(_) | Self::TimestampMillis(_) => std::mem::size_of::<i64>(),
            Self::Float(_) => std::mem::size_of::<f64>(),
            Self::Boolean(_) => std::mem::size_of::<bool>(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct IndexedNumber(f64);

impl IndexedNumber {
    fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(canonical_f64(value)))
    }

    fn from_metadata(value: &MetadataValue) -> Option<Self> {
        value.as_ordered_f64().and_then(Self::new)
    }
}

impl PartialEq for IndexedNumber {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for IndexedNumber {}

impl PartialOrd for IndexedNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IndexedNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

fn canonical_f64(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RangeIndexBound {
    Unbounded,
    NonFinite,
    Number(IndexedNumber),
}

impl RangeIndexBound {
    fn as_lower_bound(&self) -> Bound<&IndexedNumber> {
        match self {
            Self::Unbounded | Self::NonFinite => Bound::Unbounded,
            Self::Number(value) => Bound::Included(value),
        }
    }

    fn as_upper_bound(&self) -> Bound<&IndexedNumber> {
        match self {
            Self::Unbounded | Self::NonFinite => Bound::Unbounded,
            Self::Number(value) => Bound::Included(value),
        }
    }

    fn is_greater_than(self, other: Self) -> bool {
        match (self, other) {
            (Self::Number(left), Self::Number(right)) => left > right,
            (Self::Unbounded | Self::NonFinite | Self::Number(_), _) => false,
        }
    }
}

fn range_bound(field: &str, value: Option<&MetadataValue>) -> Result<RangeIndexBound> {
    let Some(value) = value else {
        return Ok(RangeIndexBound::Unbounded);
    };

    let Some(value) = value.as_ordered_f64() else {
        return Err(RetrievalKitError::InvalidRange {
            field: field.to_owned(),
        });
    };

    Ok(IndexedNumber::new(value)
        .map(RangeIndexBound::Number)
        .unwrap_or(RangeIndexBound::NonFinite))
}

fn remove_offset<K: Ord>(map: &mut BTreeMap<K, BTreeSet<usize>>, key: &K, offset: usize) {
    let should_remove = map.get_mut(key).is_some_and(|offsets| {
        offsets.remove(&offset);
        offsets.is_empty()
    });

    if should_remove {
        map.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(entries: impl IntoIterator<Item = (&'static str, MetadataValue)>) -> Metadata {
        entries
            .into_iter()
            .map(|(field, value)| (field.to_owned(), value))
            .collect()
    }

    #[test]
    fn equality_filter_returns_matching_offsets() {
        let mut index = MetadataFilterIndex::default();
        index.insert(
            0,
            &metadata([("source", MetadataValue::String("notes".to_owned()))]),
        );
        index.insert(
            1,
            &metadata([("source", MetadataValue::String("transcript".to_owned()))]),
        );

        let candidates = index
            .candidate_offsets(&Filter::Equals {
                field: "source".to_owned(),
                value: MetadataValue::String("notes".to_owned()),
            })
            .unwrap()
            .unwrap();

        assert_eq!(candidates, vec![0]);
    }

    #[test]
    fn range_filter_returns_matching_offsets() {
        let mut index = MetadataFilterIndex::default();
        index.insert(0, &metadata([("stars", MetadataValue::Integer(2))]));
        index.insert(1, &metadata([("stars", MetadataValue::Integer(5))]));
        index.insert(2, &metadata([("stars", MetadataValue::Integer(8))]));

        let candidates = index
            .candidate_offsets(&Filter::Range {
                field: "stars".to_owned(),
                lower: Some(MetadataValue::Integer(4)),
                upper: Some(MetadataValue::Integer(6)),
            })
            .unwrap()
            .unwrap();

        assert_eq!(candidates, vec![1]);
    }

    #[test]
    fn inverted_range_filter_returns_no_offsets() {
        let mut index = MetadataFilterIndex::default();
        index.insert(0, &metadata([("stars", MetadataValue::Integer(5))]));

        let candidates = index
            .candidate_offsets(&Filter::Range {
                field: "stars".to_owned(),
                lower: Some(MetadataValue::Integer(8)),
                upper: Some(MetadataValue::Integer(4)),
            })
            .unwrap()
            .unwrap();

        assert!(candidates.is_empty());
    }

    #[test]
    fn removed_offsets_do_not_match_candidates() {
        let mut index = MetadataFilterIndex::default();
        let metadata = metadata([("source", MetadataValue::String("notes".to_owned()))]);
        index.insert(0, &metadata);
        index.remove(0, &metadata);

        let candidates = index
            .candidate_offsets(&Filter::Exists {
                field: "source".to_owned(),
            })
            .unwrap()
            .unwrap();

        assert!(candidates.is_empty());
    }

    #[test]
    fn any_filter_falls_back_when_a_child_is_not_indexable() {
        let index = MetadataFilterIndex::default();

        let candidates = index
            .candidate_offsets(&Filter::Any(vec![
                Filter::Equals {
                    field: "source".to_owned(),
                    value: MetadataValue::String("notes".to_owned()),
                },
                Filter::Range {
                    field: "bad".to_owned(),
                    lower: Some(MetadataValue::Float(f64::NAN)),
                    upper: None,
                },
            ]))
            .unwrap();

        assert_eq!(candidates, None);
    }
}
