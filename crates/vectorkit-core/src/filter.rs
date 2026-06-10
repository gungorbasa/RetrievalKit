use crate::error::{Result, VectorKitError};
use crate::metadata::{Metadata, MetadataValue};

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Equals {
        field: String,
        value: MetadataValue,
    },
    NotEquals {
        field: String,
        value: MetadataValue,
    },
    In {
        field: String,
        values: Vec<MetadataValue>,
    },
    Range {
        field: String,
        lower: Option<MetadataValue>,
        upper: Option<MetadataValue>,
    },
    Exists {
        field: String,
    },
    All(Vec<Filter>),
    Any(Vec<Filter>),
}

impl Filter {
    /// Matches chunks where `field` equals `value`.
    pub fn eq(field: impl Into<String>, value: impl Into<MetadataValue>) -> Self {
        Self::Equals {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Matches chunks where `field` does not equal `value`.
    pub fn ne(field: impl Into<String>, value: impl Into<MetadataValue>) -> Self {
        Self::NotEquals {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Matches chunks where `field` equals one of `values`.
    pub fn in_values<I, V>(field: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<MetadataValue>,
    {
        Self::In {
            field: field.into(),
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    /// Matches chunks where `field` falls within an inclusive range.
    pub fn range(
        field: impl Into<String>,
        lower: Option<MetadataValue>,
        upper: Option<MetadataValue>,
    ) -> Self {
        Self::Range {
            field: field.into(),
            lower,
            upper,
        }
    }

    /// Matches chunks where `field` is greater than or equal to `lower`.
    pub fn gte(field: impl Into<String>, lower: impl Into<MetadataValue>) -> Self {
        Self::range(field, Some(lower.into()), None)
    }

    /// Matches chunks where `field` is less than or equal to `upper`.
    pub fn lte(field: impl Into<String>, upper: impl Into<MetadataValue>) -> Self {
        Self::range(field, None, Some(upper.into()))
    }

    /// Matches chunks where `field` is between `lower` and `upper`, inclusive.
    pub fn between(
        field: impl Into<String>,
        lower: impl Into<MetadataValue>,
        upper: impl Into<MetadataValue>,
    ) -> Self {
        Self::range(field, Some(lower.into()), Some(upper.into()))
    }

    /// Matches chunks that contain `field`.
    pub fn exists(field: impl Into<String>) -> Self {
        Self::Exists {
            field: field.into(),
        }
    }

    /// Matches chunks where every child filter matches.
    pub fn all(filters: impl IntoIterator<Item = Filter>) -> Self {
        Self::All(filters.into_iter().collect())
    }

    /// Matches chunks where at least one child filter matches.
    pub fn any(filters: impl IntoIterator<Item = Filter>) -> Self {
        Self::Any(filters.into_iter().collect())
    }

    pub fn matches(&self, metadata: &Metadata) -> Result<bool> {
        match self {
            Self::Equals { field, value } => Ok(metadata.get(field) == Some(value)),
            Self::NotEquals { field, value } => Ok(metadata.get(field) != Some(value)),
            Self::In { field, values } => Ok(metadata
                .get(field)
                .is_some_and(|actual| values.contains(actual))),
            Self::Range {
                field,
                lower,
                upper,
            } => range_matches(field, metadata.get(field), lower.as_ref(), upper.as_ref()),
            Self::Exists { field } => Ok(metadata.contains_key(field)),
            Self::All(filters) => {
                for filter in filters {
                    if !filter.matches(metadata)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Any(filters) => {
                for filter in filters {
                    if filter.matches(metadata)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}

fn range_matches(
    field: &str,
    actual: Option<&MetadataValue>,
    lower: Option<&MetadataValue>,
    upper: Option<&MetadataValue>,
) -> Result<bool> {
    let Some(actual) = actual.and_then(MetadataValue::as_ordered_f64) else {
        return Ok(false);
    };

    if let Some(lower) = lower {
        let Some(lower) = lower.as_ordered_f64() else {
            return Err(VectorKitError::InvalidRange {
                field: field.to_owned(),
            });
        };
        if actual < lower {
            return Ok(false);
        }
    }

    if let Some(upper) = upper {
        let Some(upper) = upper.as_ordered_f64() else {
            return Err(VectorKitError::InvalidRange {
                field: field.to_owned(),
            });
        };
        if actual > upper {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::MetadataValue;

    #[test]
    fn supports_equals_in_exists_and_range_filters() {
        let metadata = Metadata::from([
            (
                "source".to_owned(),
                MetadataValue::String("notes".to_owned()),
            ),
            ("stars".to_owned(), MetadataValue::Integer(5)),
            ("archived".to_owned(), MetadataValue::Boolean(false)),
        ]);

        assert!(Filter::Equals {
            field: "source".to_owned(),
            value: MetadataValue::String("notes".to_owned()),
        }
        .matches(&metadata)
        .unwrap());

        assert!(Filter::In {
            field: "source".to_owned(),
            values: vec![MetadataValue::String("notes".to_owned())],
        }
        .matches(&metadata)
        .unwrap());

        assert!(Filter::Exists {
            field: "archived".to_owned(),
        }
        .matches(&metadata)
        .unwrap());

        assert!(Filter::Range {
            field: "stars".to_owned(),
            lower: Some(MetadataValue::Integer(4)),
            upper: Some(MetadataValue::Integer(5)),
        }
        .matches(&metadata)
        .unwrap());
    }

    #[test]
    fn builders_match_manual_filter_variants() {
        assert_eq!(
            Filter::eq("source", "notes"),
            Filter::Equals {
                field: "source".to_owned(),
                value: MetadataValue::String("notes".to_owned())
            }
        );
        assert_eq!(
            Filter::ne("archived", true),
            Filter::NotEquals {
                field: "archived".to_owned(),
                value: MetadataValue::Boolean(true)
            }
        );
        assert_eq!(
            Filter::in_values("source", ["notes", "transcript"]),
            Filter::In {
                field: "source".to_owned(),
                values: vec![
                    MetadataValue::String("notes".to_owned()),
                    MetadataValue::String("transcript".to_owned())
                ]
            }
        );
        assert_eq!(
            Filter::exists("source"),
            Filter::Exists {
                field: "source".to_owned()
            }
        );
    }

    #[test]
    fn range_builders_are_inclusive_for_integer_and_timestamp_values() {
        let metadata = Metadata::from([
            ("stars".to_owned(), MetadataValue::integer(5)),
            (
                "start_ms".to_owned(),
                MetadataValue::timestamp_millis(60_000),
            ),
        ]);

        assert!(Filter::between("stars", 5_i64, 5_i64)
            .matches(&metadata)
            .unwrap());
        assert!(
            Filter::gte("start_ms", MetadataValue::timestamp_millis(60_000))
                .matches(&metadata)
                .unwrap()
        );
        assert!(
            Filter::lte("start_ms", MetadataValue::timestamp_millis(60_000))
                .matches(&metadata)
                .unwrap()
        );
        assert!(!Filter::between(
            "start_ms",
            MetadataValue::timestamp_millis(60_001),
            MetadataValue::timestamp_millis(70_000),
        )
        .matches(&metadata)
        .unwrap());
    }

    #[test]
    fn all_and_any_builders_compose_filters() {
        let metadata = Metadata::from([
            ("source".to_owned(), MetadataValue::string("notes")),
            ("archived".to_owned(), MetadataValue::boolean(false)),
            ("stars".to_owned(), MetadataValue::integer(5)),
        ]);

        let all = Filter::all([
            Filter::eq("source", "notes"),
            Filter::ne("archived", true),
            Filter::gte("stars", 4_i64),
        ]);
        let any = Filter::any([
            Filter::eq("source", "transcript"),
            Filter::eq("stars", 5_i64),
        ]);

        assert!(all.matches(&metadata).unwrap());
        assert!(any.matches(&metadata).unwrap());
    }

    #[test]
    fn invalid_range_bound_type_returns_filter_error() {
        let metadata = Metadata::from([("stars".to_owned(), MetadataValue::integer(5))]);

        let error = Filter::range("stars", Some(MetadataValue::string("bad-bound")), None)
            .matches(&metadata)
            .unwrap_err();

        assert_eq!(
            error,
            VectorKitError::InvalidRange {
                field: "stars".to_owned()
            }
        );
    }
}
