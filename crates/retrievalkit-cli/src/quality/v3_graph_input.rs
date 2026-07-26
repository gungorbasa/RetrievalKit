use retrievalkit_core::{FieldName, RecordType, VectorEncoding, VectorMetric};
use retrievalkit_graph::{
    Cardinality, ChunkNodeSchema, DuplicateReferencePolicy, FieldPath, GraphDatabase,
    GraphRetrievalDatabase, GraphSchema, MissingTargetPolicy, NodeType, RecordNodeSchema,
    RelationshipSchema, RelationshipType,
};
use serde_json::{json, Value};

use super::v3_ingestion::{build_graph_corpus, V3ProductionInputs};
use super::v3_schema::{
    ChunkNodeRule, GraphSchema as V3GraphSchema, RecordNodeRule, RelationshipRule,
};
use super::v3_validation::ValidatedCollection;

pub(super) fn production_schema(source: &V3GraphSchema) -> Result<GraphSchema, String> {
    if source.version != 1 {
        return Err(format!(
            "V3 graph adapter: unsupported schema version {}",
            source.version
        ));
    }
    let record_nodes = source
        .record_nodes
        .iter()
        .map(convert_record_node)
        .collect::<Result<Vec<_>, _>>()?;
    let relationships = source
        .relationships
        .iter()
        .map(convert_relationship)
        .collect::<Result<Vec<_>, _>>()?;
    let mut schema = GraphSchema::new(record_nodes).with_relationships(relationships);
    if let Some(chunk_nodes) = &source.chunk_nodes {
        schema = schema.with_chunk_nodes(convert_chunk_nodes(chunk_nodes)?);
    }
    schema
        .validate()
        .map_err(|error| format!("V3 graph adapter: production schema validation: {error}"))?;
    Ok(schema)
}

pub(super) fn build_graph_database(
    validated: &ValidatedCollection,
) -> Result<GraphDatabase, String> {
    let corpus = build_graph_corpus(validated)?;
    let database = GraphDatabase::build(corpus, production_schema(&validated.graph_schema)?)
        .map_err(|error| format!("V3 graph adapter: production graph build: {error}"))?;
    validate_graph_database(&database, validated)?;
    Ok(database)
}

pub(super) fn build_graph_retrieval_database(
    validated: &ValidatedCollection,
    encoding: VectorEncoding,
) -> Result<GraphRetrievalDatabase, String> {
    let inputs = V3ProductionInputs::from_validated(validated)?;
    let retrieval = inputs.build_database(encoding)?;
    let database =
        GraphRetrievalDatabase::build(retrieval, production_schema(&validated.graph_schema)?)
            .map_err(|error| {
                format!("V3 graph retrieval adapter: production combined build: {error}")
            })?;
    validate_graph_retrieval_database(&database, validated, encoding)?;
    Ok(database)
}

pub(super) fn validate_production_ingestion(
    validated: &ValidatedCollection,
) -> Result<Value, String> {
    let graph = build_graph_database(validated)?;
    let combined = build_graph_retrieval_database(validated, VectorEncoding::F32)?;
    if graph.graph().node_count() != combined.graph().node_count()
        || graph.graph().edge_count() != combined.graph().edge_count()
        || graph.graph().build_stats() != combined.graph().build_stats()
    {
        return Err(
            "V3 production ingestion: graph-only and combined graph shapes differ".to_owned(),
        );
    }
    Ok(json!({
        "chunks":graph.corpus().active_chunk_count(),
        "corpus_id":graph.corpus().corpus_id().as_str(),
        "dimension":validated.dimension,
        "edges":graph.graph().edge_count(),
        "nodes":graph.graph().node_count(),
        "records":graph.corpus().record_store().len(),
        "status":"valid"
    }))
}

fn validate_graph_retrieval_database(
    database: &GraphRetrievalDatabase,
    validated: &ValidatedCollection,
    encoding: VectorEncoding,
) -> Result<(), String> {
    let stats = database.graph().build_stats();
    let expected_chunks = validated
        .records
        .iter()
        .map(|record| record.chunks.len())
        .sum::<usize>();
    let expected_nodes = validated.records.len() + expected_chunks;
    if database.corpus().corpus_id().as_str() != validated.collection.corpus_id
        || database.corpus().record_store().len() != validated.records.len()
        || database.corpus().active_chunk_count() != expected_chunks
        || database.corpus().generation().get() != validated.records.len() as u64
        || database.retrieval().retrieval().vector_encoding() != encoding
        || database.retrieval().retrieval().dimension() != validated.dimension
        || database.retrieval().retrieval().metric() != VectorMetric::Cosine
        || !database.retrieval().retrieval().has_bm25()
        || stats.records != validated.records.len()
        || stats.nodes != expected_nodes
        || stats.diagnostics != 0
        || database.graph().node_count() != expected_nodes
        || database.graph().edge_count() != stats.edges
    {
        return Err(format!(
            "V3 graph retrieval adapter: combined database shape/configuration mismatch: corpus records/chunks {}/{}, stats {stats:?}",
            database.corpus().record_store().len(),
            database.corpus().active_chunk_count()
        ));
    }
    Ok(())
}

fn validate_graph_database(
    database: &GraphDatabase,
    validated: &ValidatedCollection,
) -> Result<(), String> {
    let stats = database.graph().build_stats();
    let expected_chunks = validated
        .records
        .iter()
        .map(|record| record.chunks.len())
        .sum::<usize>();
    let expected_nodes = validated.records.len() + expected_chunks;
    if database.corpus().corpus_id().as_str() != validated.collection.corpus_id
        || database.corpus().record_store().len() != validated.records.len()
        || database.corpus().active_chunk_count() != expected_chunks
        || database.corpus().generation().get() != validated.records.len() as u64
        || stats.records != validated.records.len()
        || stats.nodes != expected_nodes
        || stats.diagnostics != 0
        || database.graph().node_count() != expected_nodes
        || database.graph().edge_count() != stats.edges
    {
        return Err(format!(
            "V3 graph adapter: graph shape mismatch: corpus records/chunks {}/{}, stats {stats:?}",
            database.corpus().record_store().len(),
            database.corpus().active_chunk_count()
        ));
    }
    Ok(())
}

fn convert_record_node(source: &RecordNodeRule) -> Result<RecordNodeSchema, String> {
    Ok(RecordNodeSchema {
        record_type: RecordType::new(source.record_type.clone())
            .map_err(|error| format!("V3 graph adapter: record type: {error}"))?,
        node_type: node_type(&source.node_type)?,
        queryable_fields: source
            .queryable_fields
            .iter()
            .map(|path| field_path(path))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn convert_relationship(source: &RelationshipRule) -> Result<RelationshipSchema, String> {
    Ok(RelationshipSchema {
        relationship_type: relationship_type(&source.relationship_type)?,
        source_node_type: node_type(&source.source_node_type)?,
        target_node_type: node_type(&source.target_node_type)?,
        source_field: field_path(&source.source_field)?,
        cardinality: match source.cardinality.as_str() {
            "one" => Cardinality::One,
            "optional_one" => Cardinality::OptionalOne,
            "many" => Cardinality::Many,
            actual => {
                return Err(format!(
                    "V3 graph adapter: unsupported cardinality '{actual}'"
                ));
            }
        },
        missing_target: match source.missing_target.as_str() {
            "error" => MissingTargetPolicy::Error,
            "omit_edge" => MissingTargetPolicy::OmitEdge,
            actual => {
                return Err(format!(
                    "V3 graph adapter: unsupported missing-target policy '{actual}'"
                ));
            }
        },
        duplicate_references: match source.duplicate_references.as_str() {
            "error" => DuplicateReferencePolicy::Error,
            "deduplicate" => DuplicateReferencePolicy::Deduplicate,
            actual => {
                return Err(format!(
                    "V3 graph adapter: unsupported duplicate-reference policy '{actual}'"
                ));
            }
        },
        allow_self_edge: source.allow_self_edge,
        inverse_relationship: source
            .inverse_relationship
            .as_deref()
            .map(relationship_type)
            .transpose()?,
    })
}

fn convert_chunk_nodes(source: &ChunkNodeRule) -> Result<ChunkNodeSchema, String> {
    Ok(ChunkNodeSchema {
        node_type: node_type(&source.node_type)?,
        owns_relationship: relationship_type(&source.owns_relationship)?,
        inverse_relationship: source
            .inverse_relationship
            .as_deref()
            .map(relationship_type)
            .transpose()?,
    })
}

fn node_type(value: &str) -> Result<NodeType, String> {
    NodeType::new(value.to_owned()).map_err(|error| format!("V3 graph adapter: {error}"))
}

fn relationship_type(value: &str) -> Result<RelationshipType, String> {
    RelationshipType::new(value.to_owned()).map_err(|error| format!("V3 graph adapter: {error}"))
}

fn field_path(segments: &[String]) -> Result<FieldPath, String> {
    let fields = segments
        .iter()
        .map(|segment| {
            FieldName::new(segment.clone()).map_err(|error| format!("V3 graph adapter: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    FieldPath::new(fields).map_err(|error| format!("V3 graph adapter: {error}"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use retrievalkit_core::{ChunkIdentity, RecordId};
    use retrievalkit_graph::{Direction, GraphQuery, NodeId, QueryLimits, Seed, Traverse};

    use super::*;
    use crate::quality::v3_ingestion::V3ProductionInputs;
    use crate::quality::v3_validation::validate;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/retrieval-quality/v3")
    }

    #[test]
    fn maps_frozen_schema_and_builds_graph_only_database() {
        let validated = validate(&fixture_root()).unwrap();
        let database = build_graph_database(&validated).unwrap();

        assert_eq!(database.graph().node_count(), 15);
        assert_eq!(database.graph().edge_count(), 26);
        assert_eq!(
            database.graph().build_stats(),
            retrievalkit_graph::GraphBuildStats {
                records: 7,
                nodes: 15,
                edges: 26,
                diagnostics: 0,
            }
        );
        assert_eq!(database.corpus().active_chunk_count(), 8);
        assert_eq!(
            database.corpus().chunk_id_for_identity(&ChunkIdentity::new(
                RecordId::new("alpha").unwrap(),
                retrievalkit_core::ChunkKey::new("details").unwrap(),
            )),
            Some(0)
        );
    }

    #[test]
    fn shuffled_input_order_builds_the_same_stable_graph() {
        let validated = validate(&fixture_root()).unwrap();
        let schema = production_schema(&validated.graph_schema).unwrap();
        let inputs = V3ProductionInputs::from_validated(&validated).unwrap();
        let ordered = GraphDatabase::build(inputs.build_corpus().unwrap(), schema.clone()).unwrap();
        let mut shuffled = inputs.clone();
        shuffled.records.reverse();
        for record in &mut shuffled.records {
            record.chunks.reverse();
        }
        let shuffled = GraphDatabase::build(shuffled.build_corpus().unwrap(), schema).unwrap();

        assert_eq!(ordered.graph().node_count(), shuffled.graph().node_count());
        assert_eq!(ordered.graph().edge_count(), shuffled.graph().edge_count());
        assert_eq!(
            stable_query_result(&ordered),
            stable_query_result(&shuffled)
        );
    }

    #[test]
    fn builds_frozen_f32_and_i8_combined_databases() {
        let validated = validate(&fixture_root()).unwrap();
        for encoding in [VectorEncoding::F32, VectorEncoding::I8ScalarQuantized] {
            let database = build_graph_retrieval_database(&validated, encoding).unwrap();
            assert_eq!(database.corpus().active_chunk_count(), 8);
            assert_eq!(database.graph().node_count(), 15);
            assert_eq!(database.graph().edge_count(), 26);
            assert_eq!(database.retrieval().retrieval().vector_encoding(), encoding);
            assert!(database.retrieval().retrieval().has_bm25());
        }
    }

    #[test]
    fn rejects_malformed_schema_enum_before_graph_build() {
        let validated = validate(&fixture_root()).unwrap();
        let mut schema = validated.graph_schema.clone();
        schema.relationships[0].cardinality = "several".to_owned();

        let error = production_schema(&schema).unwrap_err();
        assert!(error.contains("unsupported cardinality 'several'"));
    }

    fn stable_query_result(
        database: &GraphDatabase,
    ) -> Vec<(NodeId, Vec<retrievalkit_graph::GraphPathEdge>)> {
        let query = GraphQuery::new(Seed::NodeIds(vec![NodeId::record(
            NodeType::new("Topic").unwrap(),
            RecordId::new("alpha").unwrap(),
        )]))
        .traverse(Traverse {
            relationship: RelationshipType::new("related").unwrap(),
            direction: Direction::Outgoing,
            min_hops: 0,
            max_hops: 1,
        })
        .with_limits(QueryLimits {
            max_hops: 3,
            max_visited: 64,
            max_results: 16,
            max_working_bytes: 65_536,
        });
        database
            .graph_query(&query, None)
            .unwrap()
            .matches
            .into_iter()
            .map(|matched| (matched.node_id, matched.path))
            .collect()
    }
}
