mod common;

use retrievalkit_core::{Filter, HybridQuery, KeywordQuery, MetadataValue, SearchQuery};
use retrievalkit_graph::{
    CancellationToken, Direction, GraphIndex, GraphQuery, GraphScalar, QueryLimits, Seed, Traverse,
    TruncationReason,
};

use common::{field, node_type, record_node, relationship, social_core, social_schema};

fn graph() -> GraphIndex {
    GraphIndex::build(social_core(false), social_schema()).unwrap()
}

#[test]
fn equality_seed_and_multistep_traversal_are_deterministic() {
    let graph = graph();
    let query = GraphQuery::new(Seed::Equals {
        node_type: node_type("Person"),
        field: retrievalkit_graph::FieldPath::single(field("name")),
        values: vec![GraphScalar::String("Alice".to_owned())],
    })
    .traverse(Traverse {
        relationship: relationship("WORKS_ON"),
        direction: Direction::Outgoing,
        min_hops: 1,
        max_hops: 1,
    })
    .traverse(Traverse {
        relationship: relationship("HAS_MEMBER"),
        direction: Direction::Outgoing,
        min_hops: 1,
        max_hops: 1,
    });
    let first = graph.graph_query(&query, None).unwrap();
    let second = graph.graph_query(&query, None).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .matches
            .iter()
            .map(|matched| matched.node_id.clone())
            .collect::<Vec<_>>(),
        vec![record_node("Person", "alice"), record_node("Person", "bob")]
    );
    assert!(first.matches.iter().all(|matched| matched.depth == 2));
    assert!(first.matches.iter().all(|matched| matched.path.len() == 2));
    assert!(first.matches.iter().all(|matched| {
        matched.path[0].provenance.source_field
            == Some(retrievalkit_graph::FieldPath::single(field("project_ids")))
            && !matched.path[0].provenance.built_in
    }));
}

#[test]
fn query_ir_round_trips_and_undeclared_property_seeds_fail() {
    let graph = graph();
    let query = GraphQuery::new(Seed::Equals {
        node_type: node_type("Person"),
        field: retrievalkit_graph::FieldPath::single(field("name")),
        values: vec![GraphScalar::String("Alice".to_owned())],
    });
    let bytes = serde_json::to_vec(&query).unwrap();
    let decoded: GraphQuery = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, query);

    let undeclared = GraphQuery::new(Seed::Equals {
        node_type: node_type("Person"),
        field: retrievalkit_graph::FieldPath::single(field("project_ids")),
        values: vec![GraphScalar::String("project-a".to_owned())],
    });
    assert!(matches!(
        graph.graph_query(&undeclared, None).unwrap_err(),
        retrievalkit_graph::GraphError::InvalidQuery { .. }
    ));
}

#[test]
fn cycles_return_shortest_results_without_looping() {
    let graph = graph();
    let query =
        GraphQuery::new(Seed::NodeIds(vec![record_node("Person", "alice")])).traverse(Traverse {
            relationship: relationship("KNOWS"),
            direction: Direction::Outgoing,
            min_hops: 1,
            max_hops: 3,
        });
    let result = graph.graph_query(&query, None).unwrap();
    assert_eq!(
        result
            .matches
            .iter()
            .map(|matched| (matched.node_id.clone(), matched.depth))
            .collect::<Vec<_>>(),
        vec![
            (record_node("Person", "bob"), 1),
            (record_node("Person", "carol"), 2),
            (record_node("Person", "alice"), 3),
        ]
    );
    assert!(result.truncated.is_none());
}

#[test]
fn incoming_traversal_and_zero_hop_steps_work() {
    let graph = graph();
    let incoming =
        GraphQuery::new(Seed::NodeIds(vec![record_node("Person", "alice")])).traverse(Traverse {
            relationship: relationship("KNOWS"),
            direction: Direction::Incoming,
            min_hops: 1,
            max_hops: 1,
        });
    let result = graph.graph_query(&incoming, None).unwrap();
    assert_eq!(result.matches[0].node_id, record_node("Person", "carol"));

    let zero_hop =
        GraphQuery::new(Seed::NodeIds(vec![record_node("Person", "alice")])).traverse(Traverse {
            relationship: relationship("KNOWS"),
            direction: Direction::Outgoing,
            min_hops: 0,
            max_hops: 0,
        });
    let result = graph.graph_query(&zero_hop, None).unwrap();
    assert_eq!(result.matches[0].node_id, record_node("Person", "alice"));
    assert_eq!(result.matches[0].depth, 0);
}

#[test]
fn caller_limits_truncate_deterministically_and_cancellation_is_typed() {
    let graph = graph();
    let query = GraphQuery::new(Seed::NodeIds(vec![record_node("Person", "alice")]))
        .traverse(Traverse {
            relationship: relationship("WORKS_ON"),
            direction: Direction::Outgoing,
            min_hops: 1,
            max_hops: 1,
        })
        .with_limits(QueryLimits {
            max_hops: 4,
            max_visited: 100,
            max_results: 1,
            max_working_bytes: 1024 * 1024,
        });
    let result = graph.graph_query(&query, None).unwrap();
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.truncated, Some(TruncationReason::MaxResults));

    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert!(matches!(
        graph.graph_query(&query, Some(&cancellation)).unwrap_err(),
        retrievalkit_graph::GraphError::Cancelled
    ));
}

#[test]
fn hop_visit_and_working_memory_limits_report_distinct_reasons() {
    let graph = graph();
    let base =
        GraphQuery::new(Seed::NodeIds(vec![record_node("Person", "alice")])).traverse(Traverse {
            relationship: relationship("KNOWS"),
            direction: Direction::Outgoing,
            min_hops: 1,
            max_hops: 3,
        });

    let hop_limited = base.clone().with_limits(QueryLimits {
        max_hops: 1,
        max_visited: 100,
        max_results: 10,
        max_working_bytes: 1024 * 1024,
    });
    assert_eq!(
        graph.graph_query(&hop_limited, None).unwrap().truncated,
        Some(TruncationReason::MaxHops)
    );

    let visit_limited = base.clone().with_limits(QueryLimits {
        max_hops: 4,
        max_visited: 1,
        max_results: 10,
        max_working_bytes: 1024 * 1024,
    });
    assert_eq!(
        graph.graph_query(&visit_limited, None).unwrap().truncated,
        Some(TruncationReason::MaxVisited)
    );

    let memory_limited = base.with_limits(QueryLimits {
        max_hops: 4,
        max_visited: 100,
        max_results: 10,
        max_working_bytes: 1,
    });
    assert_eq!(
        graph.graph_query(&memory_limited, None).unwrap().truncated,
        Some(TruncationReason::MaxWorkingBytes)
    );
}

#[test]
fn record_and_chunk_results_project_into_all_scoped_rankers() {
    let graph = graph();
    let project_query = GraphQuery::new(Seed::Equals {
        node_type: node_type("Project"),
        field: retrievalkit_graph::FieldPath::single(field("name")),
        values: vec![GraphScalar::String("Analytical Engine".to_owned())],
    });
    let result = graph.graph_query(&project_query, None).unwrap();
    let projected = graph.project_candidates(&result).unwrap();
    assert_eq!(projected.trace.resolved_chunks, 1);

    let exact = graph
        .search_in_candidates(&SearchQuery::new(vec![0.0, 1.0, 0.0], 10), &projected.scope)
        .unwrap();
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0].document_id, "project-a");

    let keyword = graph
        .keyword_search_in_candidates(
            &KeywordQuery::new("Analytical searchable", 10),
            &projected.scope,
        )
        .unwrap();
    assert_eq!(keyword[0].document_id, "project-a");

    let hybrid = graph
        .hybrid_search_in_candidates(
            &HybridQuery::new("Analytical searchable", vec![0.0, 1.0, 0.0], 10).with_filter(
                Filter::eq("kind", MetadataValue::String("project".to_owned())),
            ),
            &projected.scope,
        )
        .unwrap();
    assert_eq!(hybrid[0].document_id, "project-a");

    let chunk_query = GraphQuery::new(Seed::NodeIds(vec![record_node("Person", "alice")]))
        .traverse(Traverse {
            relationship: relationship("HAS_CHUNK"),
            direction: Direction::Outgoing,
            min_hops: 1,
            max_hops: 1,
        });
    let chunk_result = graph.graph_query(&chunk_query, None).unwrap();
    assert!(chunk_result.matches[0].path[0].provenance.built_in);
    let chunk_scope = graph.project_candidates(&chunk_result).unwrap();
    assert_eq!(chunk_scope.trace.resolved_chunks, 1);
    let exact = graph
        .search_in_candidates(
            &SearchQuery::new(vec![1.0, 0.0, 0.0], 10),
            &chunk_scope.scope,
        )
        .unwrap();
    assert_eq!(exact[0].document_id, "alice");
}
