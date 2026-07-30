use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use retrievalkit_core::{CorpusId, GenerationId};
use serde::{Deserialize, Serialize};

use crate::error::{GraphError, Result};
use crate::schema::{FieldPath, NodeType, RelationshipType};
use crate::storage::{Direction, GraphPathEdge, GraphScalar, GraphStorage, NodeId, NodeOrdinal};

const HARD_MAX_STEPS: usize = 32;
const HARD_MAX_HOPS: usize = 64;
const HARD_MAX_VISITED: usize = 1_000_000;
const HARD_MAX_RESULTS: usize = 100_000;
const HARD_MAX_WORKING_BYTES: usize = 512 * 1024 * 1024;
const STATE_LOGICAL_BYTES: usize = 48;
const PATH_EDGE_LOGICAL_BYTES: usize = std::mem::size_of::<u32>();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Seed {
    NodeIds(Vec<NodeId>),
    Equals {
        node_type: NodeType,
        field: FieldPath,
        values: Vec<GraphScalar>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Traverse {
    pub relationship: RelationshipType,
    pub direction: Direction,
    pub min_hops: usize,
    pub max_hops: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryLimits {
    pub max_hops: usize,
    pub max_visited: usize,
    pub max_results: usize,
    pub max_working_bytes: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_hops: 8,
            max_visited: 100_000,
            max_results: 10_000,
            max_working_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQuery {
    pub seed: Seed,
    pub steps: Vec<Traverse>,
    pub limits: QueryLimits,
}

impl GraphQuery {
    pub fn new(seed: Seed) -> Self {
        Self {
            seed,
            steps: Vec::new(),
            limits: QueryLimits::default(),
        }
    }

    pub fn traverse(mut self, step: Traverse) -> Self {
        self.steps.push(step);
        self
    }

    pub fn with_limits(mut self, limits: QueryLimits) -> Self {
        self.limits = limits;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TruncationReason {
    MaxHops,
    MaxVisited,
    MaxResults,
    MaxWorkingBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMatch {
    pub node_id: NodeId,
    pub depth: usize,
    pub path: Vec<GraphPathEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQueryTrace {
    pub seed_count: usize,
    pub visited_states: usize,
    pub traversed_edges: usize,
    pub result_count: usize,
    pub diagnostics: usize,
}

/// Non-overlapping graph execution stages for benchmark and trace consumers.
///
/// These timings intentionally exclude query construction, projection,
/// filtering, ranking, and hydration. Callers that need a complete total must
/// measure around their entire composed operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphExecutionTimings {
    pub seed_resolution_ns: u64,
    pub traversal_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphResult {
    pub corpus_id: CorpusId,
    pub generation: GenerationId,
    pub matches: Vec<GraphMatch>,
    pub truncated: Option<TruncationReason>,
    pub trace: GraphQueryTrace,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone)]
struct State {
    node: NodeOrdinal,
    step: usize,
    hops_in_step: usize,
    depth: usize,
    path: Vec<u32>,
}

pub(crate) fn execute(
    storage: &GraphStorage,
    corpus_id: CorpusId,
    generation: GenerationId,
    query: &GraphQuery,
    cancellation: Option<&CancellationToken>,
) -> Result<GraphResult> {
    execute_with_timings(storage, corpus_id, generation, query, cancellation)
        .map(|(result, _)| result)
}

pub(crate) fn execute_with_timings(
    storage: &GraphStorage,
    corpus_id: CorpusId,
    generation: GenerationId,
    query: &GraphQuery,
    cancellation: Option<&CancellationToken>,
) -> Result<(GraphResult, GraphExecutionTimings)> {
    validate_query(query)?;
    let seed_started = Instant::now();
    let seeds = resolve_seeds(storage, &query.seed)?;
    let seed_resolution_ns = elapsed_ns(seed_started);
    let traversal_started = Instant::now();
    let seed_count = seeds.len();
    let mut queue = VecDeque::new();
    let mut visited = BTreeSet::new();
    let mut logical_bytes = 0usize;
    let mut truncated = None;
    for seed in seeds {
        if !enqueue(
            State {
                node: seed,
                step: 0,
                hops_in_step: 0,
                depth: 0,
                path: Vec::new(),
            },
            query,
            &mut queue,
            &mut visited,
            &mut logical_bytes,
            &mut truncated,
        ) {
            break;
        }
    }

    let mut results = BTreeMap::<NodeId, GraphMatch>::new();
    let mut traversed_edges = 0usize;
    'search: while let Some(state) = queue.pop_front() {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(GraphError::Cancelled);
        }
        if state.step == query.steps.len() {
            let node_id = storage.nodes[state.node as usize].clone();
            results.entry(node_id.clone()).or_insert(GraphMatch {
                node_id,
                depth: state.depth,
                path: materialize_path(storage, &state.path),
            });
            if results.len() >= query.limits.max_results {
                if !queue.is_empty() {
                    truncated = Some(TruncationReason::MaxResults);
                }
                break;
            }
            continue;
        }

        let step = &query.steps[state.step];
        if state.hops_in_step == 0
            && step.min_hops == 0
            && !enqueue(
                State {
                    node: state.node,
                    step: state.step + 1,
                    hops_in_step: 0,
                    depth: state.depth,
                    path: state.path.clone(),
                },
                query,
                &mut queue,
                &mut visited,
                &mut logical_bytes,
                &mut truncated,
            )
        {
            break;
        }
        if state.hops_in_step >= step.max_hops {
            continue;
        }
        if state.depth >= query.limits.max_hops {
            truncated = Some(TruncationReason::MaxHops);
            continue;
        }

        for (neighbor, edge_index, _) in
            storage.neighbors(state.node, step.direction, &step.relationship)
        {
            traversed_edges += 1;
            let hops = state.hops_in_step + 1;
            let depth = state.depth + 1;
            let mut path = state.path.clone();
            path.push(edge_index);
            if hops >= step.min_hops
                && !enqueue(
                    State {
                        node: neighbor,
                        step: state.step + 1,
                        hops_in_step: 0,
                        depth,
                        path: path.clone(),
                    },
                    query,
                    &mut queue,
                    &mut visited,
                    &mut logical_bytes,
                    &mut truncated,
                )
            {
                break 'search;
            }
            if hops < step.max_hops
                && !enqueue(
                    State {
                        node: neighbor,
                        step: state.step,
                        hops_in_step: hops,
                        depth,
                        path,
                    },
                    query,
                    &mut queue,
                    &mut visited,
                    &mut logical_bytes,
                    &mut truncated,
                )
            {
                break 'search;
            }
        }
    }

    let mut matches = results.into_values().collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        left.depth
            .cmp(&right.depth)
            .then_with(|| left.node_id.cmp(&right.node_id))
    });
    let trace = GraphQueryTrace {
        seed_count,
        visited_states: visited.len(),
        traversed_edges,
        result_count: matches.len(),
        diagnostics: storage.diagnostics.len(),
    };
    let result = GraphResult {
        corpus_id,
        generation,
        matches,
        truncated,
        trace,
    };
    Ok((
        result,
        GraphExecutionTimings {
            seed_resolution_ns,
            traversal_ns: elapsed_ns(traversal_started),
        },
    ))
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn materialize_path(storage: &GraphStorage, path: &[u32]) -> Vec<GraphPathEdge> {
    path.iter()
        .filter_map(|edge_index| storage.edges.get(*edge_index as usize))
        .map(|edge| GraphPathEdge {
            edge_id: edge.id.clone(),
            provenance: edge.provenance.clone(),
        })
        .collect()
}

fn enqueue(
    state: State,
    query: &GraphQuery,
    queue: &mut VecDeque<State>,
    visited: &mut BTreeSet<(NodeOrdinal, usize, usize)>,
    logical_bytes: &mut usize,
    truncated: &mut Option<TruncationReason>,
) -> bool {
    let key = (state.node, state.step, state.hops_in_step);
    if visited.contains(&key) {
        return true;
    }
    if visited.len() >= query.limits.max_visited {
        *truncated = Some(TruncationReason::MaxVisited);
        return false;
    }
    let state_bytes = STATE_LOGICAL_BYTES
        .saturating_add(state.path.len().saturating_mul(PATH_EDGE_LOGICAL_BYTES));
    if logical_bytes.saturating_add(state_bytes) > query.limits.max_working_bytes {
        *truncated = Some(TruncationReason::MaxWorkingBytes);
        return false;
    }
    *logical_bytes = logical_bytes.saturating_add(state_bytes);
    visited.insert(key);
    queue.push_back(state);
    true
}

fn resolve_seeds(storage: &GraphStorage, seed: &Seed) -> Result<Vec<NodeOrdinal>> {
    let mut ordinals = match seed {
        Seed::NodeIds(node_ids) => node_ids
            .iter()
            .map(|node_id| {
                storage
                    .node_ordinal(node_id)
                    .ok_or_else(|| GraphError::InvalidQuery {
                        message: format!("seed node {node_id:?} is unavailable"),
                    })
            })
            .collect::<Result<Vec<_>>>()?,
        Seed::Equals {
            node_type,
            field,
            values,
        } => {
            if !storage
                .queryable_fields
                .contains(&(node_type.clone(), field.clone()))
            {
                return Err(GraphError::InvalidQuery {
                    message: format!(
                        "field {:?} is not declared queryable for node type '{}'",
                        field.segments(),
                        node_type.as_str()
                    ),
                });
            }
            values
                .iter()
                .flat_map(|value| {
                    storage
                        .properties
                        .get(&(node_type.clone(), field.clone(), value.clone()))
                        .into_iter()
                        .flatten()
                        .copied()
                })
                .collect()
        }
    };
    ordinals.sort_by_key(|ordinal| &storage.nodes[*ordinal as usize]);
    ordinals.dedup();
    Ok(ordinals)
}

fn validate_query(query: &GraphQuery) -> Result<()> {
    if query.steps.len() > HARD_MAX_STEPS {
        return Err(GraphError::QueryLimitExceeded {
            message: format!("steps {} > {HARD_MAX_STEPS}", query.steps.len()),
        });
    }
    if query.limits.max_hops > HARD_MAX_HOPS
        || query.limits.max_visited > HARD_MAX_VISITED
        || query.limits.max_results > HARD_MAX_RESULTS
        || query.limits.max_working_bytes > HARD_MAX_WORKING_BYTES
    {
        return Err(GraphError::QueryLimitExceeded {
            message: "one or more caller limits exceed hard safety caps".to_owned(),
        });
    }
    if query.limits.max_results == 0
        || query.limits.max_visited == 0
        || query.limits.max_working_bytes == 0
    {
        return Err(GraphError::QueryLimitExceeded {
            message: "visited, result, and working-byte limits must be positive".to_owned(),
        });
    }
    for step in &query.steps {
        if step.min_hops > step.max_hops || step.max_hops > HARD_MAX_HOPS {
            return Err(GraphError::QueryLimitExceeded {
                message: format!(
                    "relationship '{}' has invalid hop bounds {}..{}",
                    step.relationship.as_str(),
                    step.min_hops,
                    step.max_hops
                ),
            });
        }
    }
    Ok(())
}
