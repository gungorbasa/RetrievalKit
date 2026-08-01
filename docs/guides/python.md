# RetrievalKit for Python

RetrievalKit is one local retrieval system with two alternative retrieval
distributions and one independent embedding distribution:

- Install `retrievalkit-graph` when records have useful relationships. It
  already includes semantic and hybrid retrieval.
- Install the smaller `retrievalkit` distribution when the corpus is a flat
  collection and graph traversal would add no value.
- Install `retrievalkit-embedding` independently when the application needs
  the first-party local FP32 MiniLM provider.

The two retrieval distributions embed alternative native aggregates, so they
are mutually exclusive within one process. The embedding distribution is an
independent integration. The examples below use the same Project Apollo notes
to show how the retrieval choice changes scope, not the underlying search
model.

## Installation status

The intended public installs are:

```bash
# PENDING — v0.1.0 is unpublished; these commands describe the approved release.
python -m pip install retrievalkit
python -m pip install retrievalkit-graph
python -m pip install retrievalkit-embedding
```

Install exactly one retrieval aggregate. Choose `retrievalkit-graph` when
relationships should scope retrieval; it already contains base retrieval.
Choose `retrievalkit` for a flat corpus.

The base and graph names are reserved with `0.0.0a0` non-SDK placeholders and
trust the protected release workflow. `retrievalkit-embedding` still requires
the same one-time bootstrap and trusted-publisher setup. Do not install the
placeholders. The available SDK route today is the macOS arm64 graph source
preview linked from the
[public docs](https://retrievalkit-docs.gungorbasa.chatgpt.site). From a
repository checkout, the available graph route is:

```bash
PYTHON_BIN=python3 scripts/check-python-graph-wrapper.sh
target/python-graph-wrapper-check-venv-py*/bin/python \
  wrappers/python-graph/examples/graph_retrieval_quickstart.py
```

The initial wheel target is macOS arm64 with CPython 3.10–3.14. Ubuntu and
Windows source portability checks are CI evidence, not published wheel support.

## Choose the right path

| Your data and question | Use | Why |
|---|---|---|
| Notes belong to projects, messages belong to threads, or documents cite one another | `retrievalkit-graph` with `GraphRetrievalDatabase` | Traverse relationships to choose candidates, then rank those candidates |
| Records form a flat collection | `retrievalkit` with `RetrievalDatabase` | Get semantic and hybrid retrieval without defining a graph |
| You have query text and an embedding | Hybrid search | Meaning and exact keyword evidence can support each other |
| You have only an embedding, or wording should not matter | Semantic search | Rank by vector similarity alone |
| You need hard tenant, status, type, or date rules | Metadata filters | Filters are constraints and work with either retrieval mode |
| You only need traversal and candidate projection | `GraphDatabase` | Avoid retrieval configuration and embeddings entirely |

Hybrid search is the normal default for app and document search. Semantic-only
search is a query variation for cases where keyword evidence is unavailable or
deliberately irrelevant; it is not a different database architecture.

## Complete product: graph-scoped hybrid search

Suppose a workspace contains notes from many projects. The user asks:
“Why did we choose Swift?” while viewing Project Apollo.

The graph answers *where to search*: start at Apollo and follow `contains` to
its notes. The metadata filter requires an approved note. Hybrid retrieval then
answers *which candidate ranks first* using semantic and BM25 evidence.

Build the graph-enabled wrapper from the repository root:

```bash
PYTHON_BIN=python3 scripts/check-python-graph-wrapper.sh
```

The check begins with a preflight that requires CPython 3.10-3.14, Python
`venv`, and Rust `cargo`. It prints the detected interpreter, Rust toolchain,
and host before starting the build. The initial public wheel target remains
macOS arm64 even when source validation succeeds on another host.

Then run this checked-in program:

```bash
target/python-graph-wrapper-check-venv-py*/bin/python \
  wrappers/python-graph/examples/graph_retrieval_quickstart.py
```

Complete runnable source:

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
    record_nodes=[
        GraphRecordNode("Project", "Project", ["title"]),
        GraphRecordNode("Note", "Note", ["title"]),
    ],
    relationships=[
        GraphRelationship(
            "contains",
            "Project",
            "Note",
            "note_ids",
            "many",
        )
    ],
)

builder = GraphRetrievalDatabaseBuilder(
    corpus_id="project-notes",
    graph=schema,
    metric="dot_product",
    encoding="f32",
)
builder.upsert(
    {
        "id": "apollo",
        "record_type": "Project",
        "fields": {
            "title": "Project Apollo",
            "note_ids": ["decision-swift", "launch-checklist"],
        },
    }
)
builder.upsert(
    {
        "id": "decision-swift",
        "record_type": "Note",
        "fields": {"title": "Apple client architecture decision"},
        "content": "We chose Swift for Project Apollo's Apple platform client.",
        "metadata": {"status": "approved"},
    },
    embedding=[1.0, 0.0],
)
builder.upsert(
    {
        "id": "launch-checklist",
        "record_type": "Note",
        "fields": {"title": "Launch checklist"},
        "content": "Project Apollo launch checklist and release owners.",
        "metadata": {"status": "draft"},
    },
    embedding=[0.0, 1.0],
)
database = builder.build()

selection = database.graph.query(
    seeds=[GraphNode("Project", "apollo")],
    traversals=[GraphTraversal("contains")],
)
hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    within=selection,
    where={"status": "approved"},
    limit=1,
)

print(f"graph-hybrid={hits[0]['document_id']}")
```

Expected output:

```text
graph-hybrid=decision-swift
```

The relationship is application data: RetrievalKit validates and traverses
`note_ids`, but it does not extract or invent relationships.

No dimension, public chunk key, or embedding dictionary is required. Rust
queues the graph-only Apollo record, infers dimension from the first note
embedding, and derives the hidden searchable identity from each record.

## Graph traversal without retrieval

For graph-only traversal and stable candidate projection, run:

```bash
target/python-graph-wrapper-check-venv-py*/bin/python \
  wrappers/python-graph/examples/graph_quickstart.py
```

Its ingestion path is just a record:

```python
from retrievalkit_graph import GraphDatabaseBuilder, GraphRecordNode, GraphSchema

builder = GraphDatabaseBuilder(
    corpus_id="topics",
    schema=GraphSchema(
        record_nodes=[GraphRecordNode("Topic", "Topic", ["title"])]
    ),
)
builder.upsert(
    {
        "id": "retrieval",
        "record_type": "Topic",
        "fields": {"title": "Local retrieval"},
        "content": "Local semantic and lexical retrieval.",
        "metadata": {"tenant": "blue"},
    }
)
database = builder.build()
selection = database.graph.query_equals(
    node_type="Topic",
    field="title",
    values="Local retrieval",
)
projection = database.graph.project_candidates(
    selection,
    where={"tenant": "blue"},
)
```

Rust derives the record-content candidate identity and owns projection
filtering, ordering, and generation validation.

## Simpler product: hybrid search without a graph

If `project` is just metadata and users do not navigate relationships, use the
base distribution. Build and run its checked-in example:

```bash
PYTHON_BIN=python3 scripts/check-python-wrapper.sh
target/python-wrapper-check-venv-py*/bin/python \
  wrappers/python/examples/database_quickstart.py
```

The complete program is:

```python
from retrievalkit import (
    Document,
    RetrievalDatabaseBuilder,
)

builder = RetrievalDatabaseBuilder(
    corpus_id="project-notes",
    metric="dot_product",
    encoding="f32",
)
builder.upsert(
    Document(
        id="decision-swift",
        text="We chose Swift for Project Apollo's Apple platform client.",
        metadata={"project": "apollo", "status": "approved"},
    ),
    embedding=[1.0, 0.0],
)
builder.upsert(
    Document(
        id="launch-checklist",
        text="Project Apollo launch checklist and release owners.",
        metadata={"project": "apollo", "status": "draft"},
    ),
    embedding=[0.0, 1.0],
)
database = builder.build()
hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    limit=1,
)
print(f"hybrid={hits[0]['document_id']}")
```

Expected output:

```text
hybrid=decision-swift
```

`Document` is the common flat-corpus input. The first direct `embedding=`
fixes dimension in Rust, which also creates the hidden canonical record and
chunk identity. The older `RecordInput` plus chunk-key embedding maps remain
available for advanced applications that already own stable multi-chunk
identities.

For a hard Apollo-only constraint, add
`where={"project": "apollo", "status": "approved"}`. Use a graph selection
instead when “inside Apollo” means traversing explicit relationships rather
than comparing fields.

## Semantic-only is a query variation

Both database types expose semantic search. Reuse the database and omit query
text:

```python
# Graph-enabled database; `selection` is optional.
semantic_hits = database.retrieval.semantic_search(
    [1.0, 0.0],
    within=selection,
    where={"status": "approved"},
    limit=1,
)
```

On a base `RetrievalDatabase`, call the same method without `within`:

```python
semantic_hits = database.retrieval.semantic_search(
    [1.0, 0.0],
    where={"project": "apollo"},
    limit=1,
)
```

Choose this when there is no meaningful query text—for example, finding notes
similar to another note—or when exact terms should intentionally have no
influence.

## Traces and persistence

Hybrid hits include the fused score and an explanation of each component:

```python
hit = hits[0]
print(hit["trace"]["vector_rank"])
print(hit["trace"]["keyword_rank"])
print(hit["trace"]["matched_terms"])
print(hit["trace"]["vector_score"])
```

Save, validate, and reload the complete graph, corpus, retrieval indexes, and
metadata together:

```python
from pathlib import Path
from retrievalkit_graph import GraphRetrievalDatabase

snapshot = Path("project-notes.rk")
database.save(snapshot)
GraphRetrievalDatabase.validate(snapshot)
reloaded = GraphRetrievalDatabase.load(snapshot)
```

Use `RetrievalDatabase.save`, `validate`, and `load` for the base distribution.
Persistence, filtering, graph traversal, ranking, and trace construction all
run in the shared Rust core.

## Embeddings stay your choice

The two-dimensional vectors above make the example deterministic; they are not
a production embedding model. RetrievalKit requires one caller-provided
embedding per indexed chunk and a query embedding from the same model.

Use a local model when text must remain on the device. If your application
sends text to a remote embedding API, that embedding step is remote even
though indexing and retrieval remain local.

For lower-level build, lifecycle, and API details, see the
[`retrievalkit` wrapper reference](../../wrappers/python/README.md) and
[`retrievalkit-graph` wrapper reference](../../wrappers/python-graph/README.md).
