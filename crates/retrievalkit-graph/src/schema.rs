use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

use retrievalkit_core::{FieldName, RecordType};
use serde::{Deserialize, Serialize};

use crate::error::{GraphError, Result};

const SCHEMA_VERSION: u32 = 1;
const MAX_IDENTIFIER_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaHash([u8; 32]);

impl SchemaHash {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Display for SchemaHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

macro_rules! graph_identifier {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                validate_identifier($label, &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

graph_identifier!(NodeType, "node type");
graph_identifier!(RelationshipType, "relationship type");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FieldPath(Vec<FieldName>);

impl FieldPath {
    pub fn new(segments: Vec<FieldName>) -> Result<Self> {
        if segments.is_empty() {
            return Err(GraphError::InvalidSchema {
                message: "field paths require at least one segment".to_owned(),
            });
        }
        Ok(Self(segments))
    }

    pub fn single(field: FieldName) -> Self {
        Self(vec![field])
    }

    pub fn segments(&self) -> &[FieldName] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinality {
    One,
    OptionalOne,
    Many,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingTargetPolicy {
    Error,
    OmitEdge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateReferencePolicy {
    Error,
    Deduplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordNodeSchema {
    pub record_type: RecordType,
    pub node_type: NodeType,
    pub queryable_fields: Vec<FieldPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipSchema {
    pub relationship_type: RelationshipType,
    pub source_node_type: NodeType,
    pub target_node_type: NodeType,
    pub source_field: FieldPath,
    pub cardinality: Cardinality,
    pub missing_target: MissingTargetPolicy,
    pub duplicate_references: DuplicateReferencePolicy,
    pub allow_self_edge: bool,
    pub inverse_relationship: Option<RelationshipType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkNodeSchema {
    pub node_type: NodeType,
    pub owns_relationship: RelationshipType,
    pub inverse_relationship: Option<RelationshipType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSchema {
    pub version: u32,
    pub record_nodes: Vec<RecordNodeSchema>,
    pub relationships: Vec<RelationshipSchema>,
    pub chunk_nodes: Option<ChunkNodeSchema>,
}

impl GraphSchema {
    pub fn new(record_nodes: Vec<RecordNodeSchema>) -> Self {
        Self {
            version: SCHEMA_VERSION,
            record_nodes,
            relationships: Vec::new(),
            chunk_nodes: None,
        }
    }

    pub fn with_relationships(mut self, relationships: Vec<RelationshipSchema>) -> Self {
        self.relationships = relationships;
        self
    }

    pub fn with_chunk_nodes(mut self, chunk_nodes: ChunkNodeSchema) -> Self {
        self.chunk_nodes = Some(chunk_nodes);
        self
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_internal().map(|_| ())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let normalized = self.canonicalized()?;
        serde_json::to_vec(&normalized).map_err(|error| GraphError::InvalidSchema {
            message: format!("could not encode canonical schema: {error}"),
        })
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self> {
        let schema: Self =
            serde_json::from_slice(bytes).map_err(|error| GraphError::InvalidSchema {
                message: format!("could not decode canonical schema: {error}"),
            })?;
        let canonical = schema.canonical_bytes()?;
        if canonical != bytes {
            return Err(GraphError::InvalidSchema {
                message: "schema bytes are valid but not canonically encoded".to_owned(),
            });
        }
        Ok(schema)
    }

    pub fn schema_hash(&self) -> Result<SchemaHash> {
        let hash = blake3::hash(&self.canonical_bytes()?);
        Ok(SchemaHash::from_bytes(*hash.as_bytes()))
    }

    pub(crate) fn canonicalized(&self) -> Result<Self> {
        self.validate()?;
        let mut normalized = self.clone();
        for mapping in &mut normalized.record_nodes {
            mapping.queryable_fields.sort();
        }
        normalized.record_nodes.sort_by(|left, right| {
            left.record_type
                .cmp(&right.record_type)
                .then_with(|| left.node_type.cmp(&right.node_type))
        });
        normalized.relationships.sort_by(|left, right| {
            left.relationship_type
                .cmp(&right.relationship_type)
                .then_with(|| left.source_node_type.cmp(&right.source_node_type))
                .then_with(|| left.target_node_type.cmp(&right.target_node_type))
                .then_with(|| left.source_field.cmp(&right.source_field))
        });
        Ok(normalized)
    }

    pub(crate) fn validate_internal(&self) -> Result<ValidatedSchema<'_>> {
        if self.version != SCHEMA_VERSION {
            return Err(GraphError::InvalidSchema {
                message: format!("unsupported schema version {}", self.version),
            });
        }
        if self.record_nodes.is_empty() {
            return Err(GraphError::InvalidSchema {
                message: "at least one record-node mapping is required".to_owned(),
            });
        }

        let mut by_record_type = BTreeMap::new();
        let mut by_node_type = BTreeMap::new();
        for mapping in &self.record_nodes {
            if by_record_type
                .insert(mapping.record_type.clone(), mapping)
                .is_some()
            {
                return Err(GraphError::InvalidSchema {
                    message: format!(
                        "record type '{}' is mapped more than once",
                        mapping.record_type.as_str()
                    ),
                });
            }
            if by_node_type
                .insert(mapping.node_type.clone(), mapping)
                .is_some()
            {
                return Err(GraphError::InvalidSchema {
                    message: format!(
                        "node type '{}' is mapped more than once",
                        mapping.node_type.as_str()
                    ),
                });
            }
            let mut queryable = BTreeSet::new();
            for field in &mapping.queryable_fields {
                if !queryable.insert(field) {
                    return Err(GraphError::InvalidSchema {
                        message: format!(
                            "node type '{}' repeats a queryable field path",
                            mapping.node_type.as_str()
                        ),
                    });
                }
            }
        }

        if let Some(chunk_nodes) = &self.chunk_nodes {
            if by_node_type.contains_key(&chunk_nodes.node_type) {
                return Err(GraphError::InvalidSchema {
                    message: format!(
                        "chunk node type '{}' conflicts with a record node type",
                        chunk_nodes.node_type.as_str()
                    ),
                });
            }
        }

        let mut relationship_names = BTreeSet::new();
        if let Some(chunk_nodes) = &self.chunk_nodes {
            insert_relationship_name(&mut relationship_names, &chunk_nodes.owns_relationship)?;
            if let Some(inverse) = &chunk_nodes.inverse_relationship {
                insert_relationship_name(&mut relationship_names, inverse)?;
            }
        }
        for relationship in &self.relationships {
            if !by_node_type.contains_key(&relationship.source_node_type) {
                return Err(GraphError::InvalidSchema {
                    message: format!(
                        "relationship '{}' has unknown source node type '{}'",
                        relationship.relationship_type.as_str(),
                        relationship.source_node_type.as_str()
                    ),
                });
            }
            if !by_node_type.contains_key(&relationship.target_node_type) {
                return Err(GraphError::InvalidSchema {
                    message: format!(
                        "relationship '{}' has unknown target node type '{}'",
                        relationship.relationship_type.as_str(),
                        relationship.target_node_type.as_str()
                    ),
                });
            }
            insert_relationship_name(&mut relationship_names, &relationship.relationship_type)?;
            if let Some(inverse) = &relationship.inverse_relationship {
                insert_relationship_name(&mut relationship_names, inverse)?;
            }
        }

        Ok(ValidatedSchema {
            schema: self,
            by_record_type,
            by_node_type,
        })
    }
}

pub(crate) struct ValidatedSchema<'a> {
    pub schema: &'a GraphSchema,
    pub by_record_type: BTreeMap<RecordType, &'a RecordNodeSchema>,
    pub by_node_type: BTreeMap<NodeType, &'a RecordNodeSchema>,
}

fn insert_relationship_name(
    names: &mut BTreeSet<RelationshipType>,
    relationship: &RelationshipType,
) -> Result<()> {
    if !names.insert(relationship.clone()) {
        return Err(GraphError::InvalidSchema {
            message: format!(
                "relationship type '{}' is defined more than once",
                relationship.as_str()
            ),
        });
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    let mut bytes = value.bytes();
    let valid_first = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    let valid_rest = bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if value.len() > MAX_IDENTIFIER_BYTES || !valid_first || !valid_rest {
        return Err(GraphError::InvalidSchema {
            message: format!("{label} '{value}' must match [A-Za-z_][A-Za-z0-9_]{{0,63}}"),
        });
    }
    Ok(())
}
