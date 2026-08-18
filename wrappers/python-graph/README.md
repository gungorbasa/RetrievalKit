# RetrievalKit Graph for Python

> [RetrievalKit](../../README.md) › SDKs › Python graph aggregate

`retrievalkit-graph` contains graph-only traversal and graph-scoped exact,
BM25, and hybrid retrieval over the same canonical corpus. It already includes
base retrieval; do not install or import `retrievalkit` in the same process.

```bash
python -m pip install retrievalkit-graph==0.1.0
```

## Choose a database

Choose `GraphDatabase` for graph-only traversal or `GraphRetrievalDatabase`
when a graph selection should scope exact, BM25, or hybrid ranking. The smaller
base `retrievalkit` distribution remains graph-free.

For a human-readable Project Apollo walkthrough and decision guide, start with
the canonical [Python guide](../../docs/guides/python.md).

For source development, install this distribution instead of `retrievalkit`:

```bash
cd wrappers/python-graph
maturin develop
python examples/graph_quickstart.py
python examples/graph_retrieval_quickstart.py
```

`graph_quickstart.py` is graph-only. `graph_retrieval_quickstart.py` shows the
combined graph-plus-retrieval database. The base distribution's
`examples/database_quickstart.py` covers retrieval without graph code.

The combined builder keeps graph schema and retrieval configuration explicit:

```python
from retrievalkit_graph import (
    GraphNode,
    GraphRecordNode,
    GraphRelationship,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
    GraphTraversal,
)

schema = GraphSchema(
    record_nodes=[GraphRecordNode("Topic", "Topic", ["title"])],
    relationships=[
        GraphRelationship(
            "related_to",
            "Topic",
            "Topic",
            "related_id",
            "optional_one",
        )
    ],
)
builder = GraphRetrievalDatabaseBuilder(
    corpus_id="topics",
    graph=schema,
)
builder.upsert(
    {
        "id": "alpha",
        "record_type": "Topic",
        "fields": {"title": "Alpha"},
        "content": "Graph retrieval",
        "metadata": {"tenant": "blue"},
    },
    embedding=embedding,
)
database = builder.build()

selection = database.graph.query(
    seeds=[GraphNode("Topic", "alpha")],
    traversals=[GraphTraversal("related_to", max_hops=2)],
)
projection = database.graph.project_candidates(
    selection,
    where={"tenant": "blue"},
)
hits = database.retrieval.semantic_search(
    query_embedding,
    within=selection,
    where={"tenant": "blue"},
)
hybrid_hits = database.retrieval.hybrid_search(
    "graph retrieval",
    query_embedding,
    within=selection,
    where={"tenant": "blue"},
    alpha=0.6,
)
```

`GraphDatabaseBuilder(corpus_id=..., schema=...)` builds graph-only state and
accepts capability-neutral records through `upsert(record)`, with no embedding
parameter. `GraphRetrievalDatabaseBuilder` accepts the same simple record plus
an optional direct `embedding=`. Graph-only records may arrive before the first
searchable record; Rust queues them, infers dimension from the first embedding,
and derives hidden chunk identity from record content. Combined databases
deliberately expose `database.graph` and `database.retrieval` query namespaces
so graph traversal and semantic/hybrid retrieval remain separate capabilities.

The older `RecordInput`/public chunk-key and nested embedding-map methods remain
available as an advanced compatibility surface.

Graph queries are deterministic and bounded by `GraphQueryLimits`. Selections
are tied to the corpus generation that produced them and can scope semantic or
hybrid retrieval through `within=`. Metadata filtering is supported by both
retrieval methods through `where=`.
`database.graph.project_candidates(selection, where=...)` materializes stable
`GraphChunkIdentity(record_id, chunk_key)` values in lexical order. Rust owns
the generation/corpus checks, metadata-filter intersection, and the reported
source-node and before/after candidate counts.
Hybrid `alpha` is query-time: `1` is vector-only, `0` is BM25-only, and the
default is `0.6`.
`database.retrieval.keyword_search(text, within=selection)` performs direct,
embedding-free BM25 retrieval. `RetrievalConfiguration.bm25` configures `k1`,
`b`, and stop words for both unscoped and graph-scoped search, and the exact
configuration survives persistence.
Every retrieval hit includes effective metadata. Hybrid traces expose `alpha`,
source ranks, normalized scores, and matched terms; graph scope only constrains
the candidate set.

Databases and selections support `close()` and context managers. Graph queries
also support cooperative cancellation and second-based timeouts. Rust performs
schema validation, graph traversal, filtering, ranking, persistence, and
hydration; the Python layer only converts typed inputs and results. Graph query
requests, selections, candidate projections, and retrieval results cross PyO3
as typed values without JSON. Cold schema and ingestion remain JSON-based.

Because both packages embed native RetrievalKit core symbols, import either
`retrievalkit` or `retrievalkit_graph` in one process, not both.
