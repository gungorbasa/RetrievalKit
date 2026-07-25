# RetrievalKit Graph for Python

`retrievalkit-graph` is RetrievalKit with graph capabilities included. The
aggregate Python distribution supports graph-only databases and complete
graph-scoped semantic or hybrid retrieval. It mirrors the `RetrievalKitGraph`
Swift package; the smaller base `retrievalkit` distribution remains graph-free.

For a human-readable Project Apollo walkthrough and decision guide, start with
the canonical [Python guide](../../docs/guides/python.md).

Install this distribution instead of `retrievalkit` in a graph-enabled process:

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
    RetrievalConfiguration,
    VectorIndexConfiguration,
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
    retrieval=RetrievalConfiguration(
        semantic=VectorIndexConfiguration(dimension=384),
    ),
)
builder.upsert(
    {
        "record": {
            "id": "alpha",
            "record_type": "Topic",
            "fields": {"title": "Alpha"},
            "metadata": {"tenant": "blue"},
        },
        "chunks": [{"key": "summary", "text": "Graph retrieval"}],
    },
    embeddings={"summary": embedding},
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
parameter. `GraphRetrievalDatabaseBuilder` accepts the same record shape plus a
separate embedding map keyed by chunk key. Combined databases deliberately expose
`database.graph` and `database.retrieval` query namespaces so graph traversal
and semantic/hybrid retrieval remain separate capabilities.

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
