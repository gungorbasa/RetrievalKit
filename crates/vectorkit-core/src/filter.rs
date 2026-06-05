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
}
