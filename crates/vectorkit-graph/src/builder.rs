use std::collections::{BTreeMap, BTreeSet};

use vectorkit_core::{CorpusIndex, Record, RecordId, RecordValue};

use crate::error::{GraphError, Result};
use crate::schema::{
    Cardinality, DuplicateReferencePolicy, FieldPath, GraphSchema, MissingTargetPolicy,
    RecordNodeSchema, ValidatedSchema,
};
use crate::storage::{
    AdjacencyEntry, CsrAdjacency, EdgeId, EdgeProvenance, GraphScalar, GraphStorage, NodeId,
    NodeOrdinal, StoredEdge,
};

#[derive(Debug, Clone)]
struct UnresolvedEdge {
    id: EdgeId,
    provenance: EdgeProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphBuildStats {
    pub records: usize,
    pub nodes: usize,
    pub edges: usize,
    pub diagnostics: usize,
}

pub(crate) fn build_graph(
    core: &CorpusIndex,
    schema: &GraphSchema,
) -> Result<(GraphStorage, GraphBuildStats)> {
    let validated = schema.validate_internal()?;
    let mut nodes = build_nodes(core, &validated)?;
    nodes.sort();
    nodes.dedup();
    if nodes.len() > u32::MAX as usize {
        return Err(GraphError::InvalidSchema {
            message: "node count exceeds the u32 ordinal envelope".to_owned(),
        });
    }
    let node_ordinals = nodes
        .iter()
        .cloned()
        .enumerate()
        .map(|(ordinal, node)| (node, ordinal as NodeOrdinal))
        .collect::<BTreeMap<_, _>>();

    let mut edge_ids = Vec::new();
    let mut diagnostics = Vec::new();
    build_reference_edges(
        core,
        &validated,
        &node_ordinals,
        &mut edge_ids,
        &mut diagnostics,
    )?;
    build_chunk_edges(core, &validated, &node_ordinals, &mut edge_ids)?;
    edge_ids.sort_by(|left, right| left.id.cmp(&right.id));

    if edge_ids.len() > u32::MAX as usize {
        return Err(GraphError::InvalidSchema {
            message: "edge count exceeds the u32 ordinal envelope".to_owned(),
        });
    }
    let edges = edge_ids
        .into_iter()
        .map(|edge| {
            let source = node_ordinals[&edge.id.source];
            let target = node_ordinals[&edge.id.target];
            StoredEdge {
                id: edge.id,
                provenance: edge.provenance,
                source,
                target,
            }
        })
        .collect::<Vec<_>>();
    let forward = CsrAdjacency::build(
        nodes.len(),
        edges.iter().enumerate().map(|(edge_index, edge)| {
            (
                edge.source,
                AdjacencyEntry {
                    other: edge.target,
                    edge_index: edge_index as u32,
                },
            )
        }),
    );
    let reverse = CsrAdjacency::build(
        nodes.len(),
        edges.iter().enumerate().map(|(edge_index, edge)| {
            (
                edge.target,
                AdjacencyEntry {
                    other: edge.source,
                    edge_index: edge_index as u32,
                },
            )
        }),
    );
    let properties = build_property_index(core, &validated, &node_ordinals)?;
    let queryable_fields = validated
        .schema
        .record_nodes
        .iter()
        .flat_map(|mapping| {
            mapping
                .queryable_fields
                .iter()
                .cloned()
                .map(|field| (mapping.node_type.clone(), field))
        })
        .collect();
    let chunk_projections = build_chunk_projections(core, &validated)?;
    let stats = GraphBuildStats {
        records: core.record_store().len(),
        nodes: nodes.len(),
        edges: edges.len(),
        diagnostics: diagnostics.len(),
    };
    Ok((
        GraphStorage {
            nodes,
            node_ordinals,
            edges,
            forward,
            reverse,
            properties,
            queryable_fields,
            chunk_projections,
            diagnostics,
        },
        stats,
    ))
}

fn build_chunk_projections(
    core: &CorpusIndex,
    schema: &ValidatedSchema<'_>,
) -> Result<BTreeMap<NodeId, Vec<vectorkit_core::ChunkId>>> {
    let mut projections = BTreeMap::<NodeId, Vec<vectorkit_core::ChunkId>>::new();
    for (identity, chunk_id) in core.chunk_identities() {
        let record = core
            .record_store()
            .get(&identity.record_id)
            .ok_or_else(|| GraphError::InvalidRecord {
                record_id: identity.record_id.as_str().to_owned(),
                message: "active chunk mapping has no canonical record".to_owned(),
            })?;
        let mapping = schema
            .by_record_type
            .get(&record.record_type)
            .ok_or_else(|| GraphError::InvalidRecord {
                record_id: record.id.as_str().to_owned(),
                message: "chunk owner record type has no node mapping".to_owned(),
            })?;
        projections
            .entry(NodeId::record(mapping.node_type.clone(), record.id.clone()))
            .or_default()
            .push(chunk_id);
        if let Some(chunk_schema) = &schema.schema.chunk_nodes {
            projections.insert(
                NodeId::chunk(chunk_schema.node_type.clone(), identity.clone()),
                vec![chunk_id],
            );
        }
    }
    for chunk_ids in projections.values_mut() {
        chunk_ids.sort_unstable();
        chunk_ids.dedup();
    }
    Ok(projections)
}

fn build_nodes(core: &CorpusIndex, schema: &ValidatedSchema<'_>) -> Result<Vec<NodeId>> {
    let mut nodes = Vec::new();
    for (_, record) in core.record_store().iter() {
        let mapping = schema
            .by_record_type
            .get(&record.record_type)
            .ok_or_else(|| GraphError::InvalidRecord {
                record_id: record.id.as_str().to_owned(),
                message: format!(
                    "record type '{}' has no node mapping",
                    record.record_type.as_str()
                ),
            })?;
        nodes.push(NodeId::record(mapping.node_type.clone(), record.id.clone()));
    }
    if let Some(chunk_schema) = &schema.schema.chunk_nodes {
        nodes.extend(
            core.chunk_identities().map(|(identity, _)| {
                NodeId::chunk(chunk_schema.node_type.clone(), identity.clone())
            }),
        );
    }
    Ok(nodes)
}

fn build_reference_edges(
    core: &CorpusIndex,
    schema: &ValidatedSchema<'_>,
    nodes: &BTreeMap<NodeId, NodeOrdinal>,
    edges: &mut Vec<UnresolvedEdge>,
    diagnostics: &mut Vec<String>,
) -> Result<()> {
    for (rule_index, relationship) in schema.schema.relationships.iter().enumerate() {
        let source_mapping = schema.by_node_type[&relationship.source_node_type];
        let target_mapping = schema.by_node_type[&relationship.target_node_type];
        for (_, record) in core.record_store().iter() {
            if record.record_type != source_mapping.record_type {
                continue;
            }
            let references = extract_references(record, relationship)?;
            let mut seen = BTreeSet::new();
            for (occurrence, target_record_id) in references {
                if !seen.insert(target_record_id.clone()) {
                    match relationship.duplicate_references {
                        DuplicateReferencePolicy::Error => {
                            return Err(GraphError::InvalidRecord {
                                record_id: record.id.as_str().to_owned(),
                                message: format!(
                                    "relationship '{}' repeats target '{}'",
                                    relationship.relationship_type.as_str(),
                                    target_record_id.as_str()
                                ),
                            });
                        }
                        DuplicateReferencePolicy::Deduplicate => continue,
                    }
                }

                let source =
                    NodeId::record(relationship.source_node_type.clone(), record.id.clone());
                let target = NodeId::record(
                    relationship.target_node_type.clone(),
                    target_record_id.clone(),
                );
                if let Some(target_record) = core.record_store().get(&target_record_id) {
                    if target_record.record_type != target_mapping.record_type {
                        return Err(GraphError::InvalidRecord {
                            record_id: record.id.as_str().to_owned(),
                            message: format!(
                                "relationship '{}' target '{}' has record type '{}', expected '{}'",
                                relationship.relationship_type.as_str(),
                                target_record_id.as_str(),
                                target_record.record_type.as_str(),
                                target_mapping.record_type.as_str()
                            ),
                        });
                    }
                }
                let valid_target = core
                    .record_store()
                    .get(&target_record_id)
                    .is_some_and(|target| target.record_type == target_mapping.record_type)
                    && nodes.contains_key(&target);
                if !valid_target {
                    match relationship.missing_target {
                        MissingTargetPolicy::Error => {
                            return Err(GraphError::MissingTarget {
                                relationship: relationship.relationship_type.as_str().to_owned(),
                                source_record_id: record.id.as_str().to_owned(),
                                target_record_id: target_record_id.as_str().to_owned(),
                            });
                        }
                        MissingTargetPolicy::OmitEdge => {
                            diagnostics.push(format!(
                                "omitted {} from {} to missing {}",
                                relationship.relationship_type.as_str(),
                                record.id.as_str(),
                                target_record_id.as_str()
                            ));
                            continue;
                        }
                    }
                }
                if source == target && !relationship.allow_self_edge {
                    return Err(GraphError::InvalidRecord {
                        record_id: record.id.as_str().to_owned(),
                        message: format!(
                            "relationship '{}' does not allow self edges",
                            relationship.relationship_type.as_str()
                        ),
                    });
                }
                let occurrence_ordinal =
                    u32::try_from(occurrence).map_err(|_| GraphError::InvalidRecord {
                        record_id: record.id.as_str().to_owned(),
                        message: "reference occurrence exceeds u32".to_owned(),
                    })?;
                let provenance = EdgeProvenance {
                    schema_rule_index: rule_index as u32,
                    source_record_id: record.id.clone(),
                    source_field: Some(relationship.source_field.clone()),
                    derived_inverse: false,
                    built_in: false,
                };
                edges.push(UnresolvedEdge {
                    id: EdgeId {
                        relationship_type: relationship.relationship_type.clone(),
                        source: source.clone(),
                        target: target.clone(),
                        occurrence_ordinal,
                    },
                    provenance: provenance.clone(),
                });
                if let Some(inverse) = &relationship.inverse_relationship {
                    edges.push(UnresolvedEdge {
                        id: EdgeId {
                            relationship_type: inverse.clone(),
                            source: target,
                            target: source,
                            occurrence_ordinal,
                        },
                        provenance: EdgeProvenance {
                            derived_inverse: true,
                            ..provenance
                        },
                    });
                }
            }
        }
    }
    Ok(())
}

fn build_chunk_edges(
    core: &CorpusIndex,
    schema: &ValidatedSchema<'_>,
    nodes: &BTreeMap<NodeId, NodeOrdinal>,
    edges: &mut Vec<UnresolvedEdge>,
) -> Result<()> {
    let Some(chunk_schema) = &schema.schema.chunk_nodes else {
        return Ok(());
    };
    let mut ordinals = BTreeMap::<RecordId, u32>::new();
    for (identity, _) in core.chunk_identities() {
        let record = core
            .record_store()
            .get(&identity.record_id)
            .ok_or_else(|| GraphError::InvalidRecord {
                record_id: identity.record_id.as_str().to_owned(),
                message: "active chunk mapping has no canonical record".to_owned(),
            })?;
        let mapping = schema
            .by_record_type
            .get(&record.record_type)
            .ok_or_else(|| GraphError::InvalidRecord {
                record_id: record.id.as_str().to_owned(),
                message: "chunk owner record type has no node mapping".to_owned(),
            })?;
        let source = NodeId::record(mapping.node_type.clone(), record.id.clone());
        let target = NodeId::chunk(chunk_schema.node_type.clone(), identity.clone());
        if !nodes.contains_key(&source) || !nodes.contains_key(&target) {
            return Err(GraphError::InvalidSchema {
                message: "chunk relationship references an unpublished node".to_owned(),
            });
        }
        let occurrence = ordinals.entry(record.id.clone()).or_default();
        let provenance = EdgeProvenance {
            schema_rule_index: schema.schema.relationships.len() as u32,
            source_record_id: record.id.clone(),
            source_field: None,
            derived_inverse: false,
            built_in: true,
        };
        edges.push(UnresolvedEdge {
            id: EdgeId {
                relationship_type: chunk_schema.owns_relationship.clone(),
                source: source.clone(),
                target: target.clone(),
                occurrence_ordinal: *occurrence,
            },
            provenance: provenance.clone(),
        });
        if let Some(inverse) = &chunk_schema.inverse_relationship {
            edges.push(UnresolvedEdge {
                id: EdgeId {
                    relationship_type: inverse.clone(),
                    source: target,
                    target: source,
                    occurrence_ordinal: *occurrence,
                },
                provenance: EdgeProvenance {
                    derived_inverse: true,
                    ..provenance
                },
            });
        }
        *occurrence = occurrence.saturating_add(1);
    }
    Ok(())
}

fn extract_references(
    record: &Record,
    relationship: &crate::schema::RelationshipSchema,
) -> Result<Vec<(usize, RecordId)>> {
    let value = field_value(record, &relationship.source_field);
    match (relationship.cardinality, value) {
        (Cardinality::One, None | Some(RecordValue::Null)) => Err(GraphError::InvalidRecord {
            record_id: record.id.as_str().to_owned(),
            message: format!(
                "relationship '{}' requires one reference",
                relationship.relationship_type.as_str()
            ),
        }),
        (Cardinality::OptionalOne | Cardinality::Many, None | Some(RecordValue::Null)) => {
            Ok(Vec::new())
        }
        (Cardinality::One | Cardinality::OptionalOne, Some(RecordValue::String(value))) => Ok(
            vec![(0, RecordId::new(value.clone()).map_err(GraphError::from)?)],
        ),
        (Cardinality::Many, Some(RecordValue::List(values))) => values
            .iter()
            .enumerate()
            .map(|(ordinal, value)| match value {
                RecordValue::String(value) => Ok((
                    ordinal,
                    RecordId::new(value.clone()).map_err(GraphError::from)?,
                )),
                _ => Err(GraphError::InvalidRecord {
                    record_id: record.id.as_str().to_owned(),
                    message: format!(
                        "relationship '{}' requires a list of RecordId strings",
                        relationship.relationship_type.as_str()
                    ),
                }),
            })
            .collect(),
        _ => Err(GraphError::InvalidRecord {
            record_id: record.id.as_str().to_owned(),
            message: format!(
                "relationship '{}' field value does not match its cardinality",
                relationship.relationship_type.as_str()
            ),
        }),
    }
}

fn build_property_index(
    core: &CorpusIndex,
    schema: &ValidatedSchema<'_>,
    nodes: &BTreeMap<NodeId, NodeOrdinal>,
) -> Result<BTreeMap<(crate::schema::NodeType, FieldPath, GraphScalar), Vec<NodeOrdinal>>> {
    let mut properties = BTreeMap::<_, Vec<NodeOrdinal>>::new();
    for (_, record) in core.record_store().iter() {
        let mapping: &&RecordNodeSchema = schema
            .by_record_type
            .get(&record.record_type)
            .ok_or_else(|| GraphError::InvalidRecord {
                record_id: record.id.as_str().to_owned(),
                message: "record type has no node mapping".to_owned(),
            })?;
        let node_id = NodeId::record(mapping.node_type.clone(), record.id.clone());
        let ordinal = nodes[&node_id];
        for path in &mapping.queryable_fields {
            let Some(value) = field_value(record, path) else {
                continue;
            };
            let scalar = match value {
                RecordValue::Null => continue,
                RecordValue::String(value) => GraphScalar::String(value.clone()),
                RecordValue::I64(value) => GraphScalar::I64(*value),
                RecordValue::Bool(value) => GraphScalar::Bool(*value),
                _ => {
                    return Err(GraphError::InvalidRecord {
                        record_id: record.id.as_str().to_owned(),
                        message: "queryable graph properties support String, I64, and Bool"
                            .to_owned(),
                    });
                }
            };
            properties
                .entry((mapping.node_type.clone(), path.clone(), scalar))
                .or_default()
                .push(ordinal);
        }
    }
    Ok(properties)
}

fn field_value<'a>(record: &'a Record, path: &FieldPath) -> Option<&'a RecordValue> {
    let mut segments = path.segments().iter();
    let first = segments.next()?;
    let mut value = record.fields.get(first)?;
    for segment in segments {
        let RecordValue::Map(map) = value else {
            return None;
        };
        value = map.get(segment)?;
    }
    Some(value)
}
