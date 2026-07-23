# Rust HNSW Vector Search Plan

## Current Status

HNSW is deferred.

The current product direction is the small-index local MVP defined in [`../product/retrievalkit-product-spec.md`](../product/retrievalkit-product-spec.md):

```text
target size: fewer than 50K chunks
primary engine: exact vector search
retrieval: exact vector + BM25 + hybrid ranking
priority: speed, correctness, filtering, persistence, Swift/iOS integration
```

Use this HNSW plan only after the exact/hybrid engine is polished and real on-device benchmarks show exact search cannot meet the latency target for real datasets.

## Goal

Build a Rust-based vector search system using HNSW, suitable for semantic search over documents, products, notes, or other embedded content.

The system should support:

- Ingesting documents and vectors
- Building an approximate nearest neighbor index
- Running top-k vector search
- Returning matched documents and metadata
- Persisting the index and document store
- Benchmarking recall, latency, and memory usage

## Recommended Approach

Use Rust with an HNSW library and a separate storage layer for documents, metadata, and raw vectors.

For the first prototype, use one of these crates:

| Crate | Best For | Notes |
|---|---|---|
| `fast_hnsw` | Clean pure-Rust prototype | Supports common metrics, labels, payloads, persistence, and mmap loading |
| `hnswlib-rs` | Production-shaped architecture | Separates HNSW graph from vector storage |
| `hnsw_rs` | Established Rust HNSW implementation | Supports insertion, parallel search, and dump/load flows |
| `usearch` | Higher performance | Native-backed, SIMD-oriented, better if benchmarks require speed |

Default recommendation:

```text
Start with fast_hnsw.
Move to usearch only if benchmarks show that pure Rust performance is not enough.
```

## Core Architecture

The system should have four layers.

## 1. Embedding Layer

This layer converts input content into fixed-size vectors.

Input examples:

- Text documents
- Notes
- Product descriptions
- Image captions or metadata
- User queries

Output:

```rust
Vec<f32>
```

Important rules:

- All vectors must have the same dimension.
- Common dimensions are `384`, `768`, `1024`, or `1536`.
- Use cosine similarity for semantic text search.
- Normalize vectors if using cosine or dot-product similarity and the embedding model does not already normalize them.

Possible embedding sources:

- Local model via `fastembed`, `candle`, or ONNX
- External embedding API
- Precomputed embeddings from JSON, CSV, or binary files

## 2. Storage Layer

The HNSW index is not enough by itself. It can find nearby vectors, but the app still needs to return real documents and metadata.

Store:

- Document ID
- Original text or content
- Metadata
- Vector
- Optional filter fields

Prototype storage options:

- JSONL files
- SQLite
- `sled`
- Simple binary vector files

Production storage options:

- SQLite or Postgres for metadata
- Binary or mmap vector files for raw vectors
- HNSW snapshot file for the graph

Recommended layout:

```text
data/
  hnsw.index
  vectors.f32
  documents.sqlite
```

## 3. HNSW Index Layer

This layer owns the approximate nearest neighbor graph.

Responsibilities:

- Insert vectors
- Search nearest vectors
- Save/load the index
- Tune HNSW parameters
- Map vector IDs back to document IDs

Key HNSW parameters:

| Parameter | Meaning | Starting Value |
|---|---|---|
| `M` | Graph connectivity | `16` |
| `ef_construction` | Build-time accuracy/speed tradeoff | `200` |
| `ef_search` | Query-time accuracy/latency tradeoff | `80` |
| `k` | Number of final results | `10` |

Higher values usually improve recall but increase memory usage, build time, or query latency.

## 4. Search API Layer

Expose search functionality through a CLI first, then an HTTP API.

Core operations:

```text
POST /documents
POST /search
GET  /documents/:id
POST /rebuild-index
GET  /health
```

Example search request:

```json
{
  "query": "rust vector database",
  "top_k": 10,
  "ef_search": 80
}
```

Example response:

```json
{
  "results": [
    {
      "id": 123,
      "score": 0.92,
      "text": "Rust HNSW vector search...",
      "metadata": {
        "source": "notes"
      }
    }
  ]
}
```

## Implementation Plan

## Phase 1: Minimal CLI Prototype

Create a Rust project:

```bash
cargo new vector-search-hnsw
cd vector-search-hnsw
```

Add dependencies:

```toml
[dependencies]
fast_hnsw = "1"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Build a CLI that:

1. Loads sample documents.
2. Reads or generates vectors.
3. Builds an HNSW index.
4. Runs top-k search.
5. Prints IDs, distances, and document text.

Example shape:

```rust
use fast_hnsw::{Builder, Hnsw};
use fast_hnsw::distance::Cosine;

let mut index: Hnsw<Cosine> = Builder::new()
    .m(16)
    .ef_construction(200)
    .seed(42)
    .build(Cosine);

for vector in vectors {
    index.insert(vector);
}

let results = index.search(&query_vector, 10, 80);
```

## Phase 2: Define the Document Model

Use a document model like:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: u64,
    pub text: String,
    pub metadata: serde_json::Value,
    pub embedding: Vec<f32>,
}
```

Maintain an explicit mapping:

```text
hnsw_node_id -> document_id -> document metadata/text
```

If the selected HNSW crate supports labeled inserts, store the document ID directly as the label.

## Phase 3: Add Real Embeddings

For a realistic semantic search system, replace fake vectors with embeddings.

Options:

- Use a local embedding model.
- Call an external embedding API.
- Load precomputed embeddings from a file.

For text search:

```text
metric: cosine
vector type: f32
normalization: yes, unless already normalized by the model
```

## Phase 4: Two-Stage Search

Use HNSW for fast candidate retrieval, then exact reranking for better quality.

Flow:

```text
query text
  -> query embedding
  -> HNSW top 50 candidates
  -> exact cosine rerank
  -> final top 10 documents
```

Why this helps:

- HNSW is approximate.
- It may miss the perfect order.
- Exact reranking over a small candidate set improves final quality with little cost.

Recommended starting values:

```text
hnsw_candidates: 50
final_top_k: 10
ef_search: 80
```

## Phase 5: Persistence

Persist three separate things:

1. HNSW graph/index
2. Raw vectors
3. Document metadata

Prototype layout:

```text
data/
  index.bin
  vectors.bin
  documents.jsonl
```

Production layout:

```text
data/
  hnsw.index
  vectors.f32
  metadata.sqlite
```

Important note:

Some HNSW libraries persist only the graph, not the raw vectors. In that case, vectors must be saved separately.

## Phase 6: HTTP API

After the CLI works, add a service API with `axum`.

Dependencies:

```toml
[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
```

API responsibilities:

- Accept new documents
- Generate or accept embeddings
- Insert vectors into HNSW
- Search by text or vector
- Return ranked documents
- Rebuild the index if needed

## Phase 7: Benchmarking and Tuning

Benchmark before tuning aggressively.

Measure:

- Indexing throughput: vectors per second
- Query latency: p50, p95, p99
- Recall@k against brute force search
- Memory usage
- Index load time
- Index save time

Suggested benchmark configurations:

```text
M = 16, ef_construction = 200, ef_search = 50
M = 16, ef_construction = 400, ef_search = 100
M = 32, ef_construction = 400, ef_search = 100
```

Compare HNSW results against brute force exact cosine search:

```text
recall@10 = number of exact top-10 items found by HNSW / 10
```

## Recommended Defaults

Start with:

```text
metric: cosine
vector_type: f32
M: 16
ef_construction: 200
ef_search: 80
candidate_count: 50
top_k: 10
```

Then tune from real data and benchmark results.

## Project Milestones

## Milestone 1: CLI Search

Deliverables:

- Rust CLI project
- In-memory HNSW index
- Sample documents
- Top-k search command

Success criteria:

- Can index sample vectors
- Can query nearest neighbors
- Can print matching documents

## Milestone 2: Persistent Local Index

Deliverables:

- Saved HNSW index
- Saved vector store
- Saved document metadata
- Load existing index on startup

Success criteria:

- App can restart without rebuilding from scratch

## Milestone 3: Real Embeddings

Deliverables:

- Embedding provider
- Vector normalization
- Query embedding generation

Success criteria:

- Search works with real natural-language queries

## Milestone 4: API Server

Deliverables:

- `POST /documents`
- `POST /search`
- `GET /documents/:id`
- Basic health endpoint

Success criteria:

- External clients can insert and search documents

## Milestone 5: Benchmark Report

Deliverables:

- Brute force baseline
- HNSW benchmark runner
- Recall/latency comparison
- Recommended tuning values

Success criteria:

- We know the best HNSW settings for the real dataset

## Risks and Decisions

## Deletion and Updates

HNSW indexes are often better at insertion than deletion. For updates:

```text
mark old document inactive
insert new vector
filter inactive documents from results
periodically rebuild the index
```

## Metadata Filtering

Do not strict-prefilter HNSW traversal in the first version. If HNSW only walks through nodes that match a filter, recall can drop because useful graph paths may pass through non-matching nodes.

Use a query planner instead:

```text
if filter exists:
    build/evaluate matching ID bitset

    if matching_count * dimension <= exact_scan_budget:
        exact scan only matching IDs
    else if filter maps to a major partition with its own index:
        search that partition HNSW index
    else:
        global HNSW overfetch -> postfilter -> exact rerank
```

Good partition index candidates:

- tenant ID
- workspace ID
- account ID
- language
- source type

Start with:

```text
exact filtered search for small filtered sets
global HNSW overfetch + postfilter for large filtered sets
partition HNSW only for stable high-value filters
```

## Memory Usage

HNSW can use significant memory for large datasets.

Memory depends on:

- Number of vectors
- Vector dimension
- `M`
- Index implementation
- Whether raw vectors are stored in memory

For large datasets, benchmark early with realistic vector counts.

## Accuracy

Approximate search is not guaranteed to return exact nearest neighbors.

Use:

- Higher `ef_search`
- Higher `ef_construction`
- Exact reranking
- Recall@k benchmarks

## Final Recommendation

Build the first version as a CLI with:

```text
Rust
fast_hnsw
cosine similarity
JSONL or SQLite metadata
binary vector storage
two-stage retrieval
benchmark against brute force
```

Once that works, wrap it with an `axum` HTTP API and tune the HNSW parameters using real data.

## iOS On-Device Latency Target

Target:

```text
search latency: 5-10 ms
platform: iOS device
runtime: on-device
priority: search speed
```

This target changes the design. The search path must avoid database reads, heavy allocations, JSON parsing, and broad reranking during each query.

## Speed-First iOS Architecture

Recommended query path:

```text
query embedding
  -> normalized f32 vector
  -> in-memory or mmap HNSW index
  -> top K ids
  -> direct metadata lookup
  -> return results
```

Avoid this in the hot path:

```text
query
  -> HNSW anchors
  -> DB lookup for precomputed neighbors
  -> dedupe
  -> load many vectors
  -> rerank
  -> return
```

The neighbor-expansion approach can improve recall, but it usually adds more work than plain HNSW. For a 5-10 ms target, start with direct HNSW search and only add expansion if quality is not acceptable.

## iOS Search-Time Rules

Keep these true for the hot path:

- The HNSW index is already loaded before search.
- Vectors are stored contiguously.
- Metadata needed for results is in memory or in a compact mmap file.
- No SQLite query is required to produce the initial results.
- No JSON decoding happens during search.
- No network call happens during search.
- The query vector is already embedded before measuring vector search latency.
- `ef_search` is low.
- `top_k` is small, usually `5-10`.
- Result objects are small.

## Recommended iOS HNSW Settings

Start with:

```text
metric: cosine or dot product
vector normalization: yes
vector_type: f32
dimension: as small as quality allows
M: 8 or 16
ef_construction: 100-200
ef_search: 10-40
top_k: 5-10
```

For speed, `ef_search` matters most at query time. Lower values are faster but reduce recall.

Suggested benchmark grid:

```text
M = 8,  ef_search = 10
M = 8,  ef_search = 20
M = 16, ef_search = 20
M = 16, ef_search = 40
```

Pick the fastest setting that gives acceptable result quality.

## Precomputed Neighbors on iOS

Precomputed neighbors can still be useful, but not as the default fastest path.

Use them for:

- Related items
- Similar document suggestions
- Fallback expansion when low-`ef_search` HNSW quality is weak
- Offline recommendations

Do not use them as the default semantic-search hot path unless benchmarks prove it is faster than direct HNSW.

If tested, use this shape:

```text
query embedding
  -> HNSW top 1-3 anchors with very low ef_search
  -> read neighbor lists from memory, not database
  -> rerank at most 50-200 candidates
  -> return top K
```

Important constraint:

```text
neighbor lists must be in memory or mmap-backed arrays
```

If neighbor lookup requires SQLite or random disk reads, it is unlikely to beat direct HNSW for a 5-10 ms target.

## iOS Storage Layout

Use files designed for fast loading and direct lookup:

```text
Bundle or app data directory:
  index.hnsw
  vectors.f32
  metadata.compact
  id_map.bin
```

The hot path should use integer offsets:

```text
hnsw_result_internal_id -> document_offset -> compact metadata
```

For metadata, prefer compact binary records or pre-decoded in-memory structs over JSON.

## Rust and iOS Integration

Recommended shape:

```text
Rust static library
  -> C ABI wrapper
  -> Swift wrapper
  -> iOS app
```

Rust owns:

- Loading the index
- Holding search state
- Running HNSW search
- Returning result IDs and scores

Swift owns:

- UI
- Query text input
- Calling embedding model if implemented in Swift/Core ML
- Displaying result metadata

FFI boundary should be small:

```c
search_index(
  const float *query,
  size_t dim,
  uint32_t top_k,
  SearchResult *out_results
)
```

Avoid returning heap-allocated strings across FFI in the search hot path.

## Benchmark Contract

Measure these separately:

```text
embedding latency
vector search latency
metadata fetch latency
end-to-end UI latency
```

For the 5-10 ms goal, the vector search budget should probably be:

```text
HNSW search: 1-5 ms
metadata lookup: 1-2 ms
Swift/Rust boundary: <1 ms
buffer/result formatting: <1 ms
```

Benchmark on real devices, not only simulator.

Minimum benchmark matrix:

```text
dataset sizes: 10k, 50k, 100k, 500k
dimensions: 384, 768, 1536
top_k: 5, 10
ef_search: 10, 20, 40
```

The right architecture depends heavily on vector count and dimension. A 10k-vector index and a 500k-vector index are very different on iOS.

## iOS-Specific Recommendation

For the 5-10 ms goal:

```text
1. Start with direct HNSW search.
2. Keep the index resident in memory or mmap.
3. Use low ef_search.
4. Keep top_k small.
5. Avoid database reads during search.
6. Avoid precomputed-neighbor expansion at first.
7. Benchmark on device.
8. Add neighbor expansion only if it improves speed at acceptable quality.
```

If pure Rust HNSW is not fast enough, test a SIMD/native-backed implementation such as USearch. Its Rust crate describes it as a fast vector search library, and the upstream project documents bindings across Rust, Objective-C, and Swift, which makes it a practical candidate for an iOS deployment.
