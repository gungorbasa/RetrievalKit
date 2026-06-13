# Social Network MiniLM Swift Search Report

## Setup

| Field | Value |
|---|---|
| Model | `sentence-transformers/all-MiniLM-L6-v2` |
| Core ML export | `target/embedding-models/all-MiniLM-L6-v2` |
| Sequence length | `256` |
| Dimension | `384` |
| Vector encoding | `i8` |
| Metric | `cosine` |
| Source corpus | `/Users/gungorbasa/Desktop/the_social_network_v.1.32.json` |
| Persisted index | `target/examples/social-network-index-minilm` |
| Query fixture | `target/examples/social-network-minilm-queries.json` |
| Chunks | `28,650` |
| Top K | `5` |
| Warmup queries | `50` |
| Measured queries | `750` |

## Commands

Build the MiniLM-backed persisted index and query fixture:

```bash
PYTHONPATH=wrappers/python/python \
target/embedding-conversion-venv/bin/python \
  scripts/embedding/build-minilm-social-network-index.py \
  --index-dir target/examples/social-network-index-minilm \
  --queries-path target/examples/social-network-minilm-queries.json \
  --warmup-queries 50 \
  --measured-queries 750 \
  --limit 5
```

Run Swift exact vector search over the persisted index:

```bash
cd wrappers/swift/VectorKitBench
.build/release/vectorkit-bench \
  --real-index ../../../target/examples/social-network-index-minilm \
  --query-embeddings ../../../target/examples/social-network-minilm-queries.json
```

## Build Output

| Phase | Time |
|---|---:|
| Record prep | `21,314.501 ms` |
| Core ML document embedding | `118,834.370 ms` |
| VectorKit index add | `278,908.603 ms` |
| Save index | `4,409.619 ms` |
| Full build | `402,297.322 ms` |
| Query fixture embedding | `2,391.703 ms` |
| Persisted index size | `31.346 MiB` |

## Query-Time Results

| Phase | Avg | P50 | P95 | P99 | Min | Max | Samples |
|---|---:|---:|---:|---:|---:|---:|---:|
| Python Core ML MiniLM `seq=256` embedding | `3.057 ms` | `2.973 ms` | `3.545 ms` | `5.493 ms` | `2.770 ms` | `16.799 ms` | `750` |
| Swift exact vector search | `0.470 ms` | `0.466 ms` | `0.497 ms` | `0.535 ms` | `0.445 ms` | `0.633 ms` | `750` |
| Approx embedding + Swift search | `3.527 ms` | `3.439 ms` | `4.042 ms` | `6.028 ms` | `3.215 ms` | `17.432 ms` | `750` |

The approximate combined row adds independently measured embedding and Swift
search latency distributions. It is directionally useful, but the final app
number should be measured inside a single Swift end-to-end path once Swift-side
tokenization/model execution is wired into the benchmark.

## Sample Swift Search Results

| Query | Top Document | Score |
|---|---|---:|
| Location info for a Harvard clubhouse party and dorm room | `scene:scene_062:location_description` | `0.642` |
| Temporal info for one autumn night in October 2003 | `scene:scene_061:temporal_description` | `0.648` |
| Emotional landscape with dramatic irony | `scene:scene_007:emotions_description` | `0.654` |
| Visual montage across party and hacking scenes | `shot:sequence_6443.7_6451.2_objects_chunk_4` | `0.687` |
| Night exterior outside a brick building entrance | `scene:scene_007:video_description` | `0.707` |

## Takeaways

- MiniLM `seq=256` search over the real 28,650 chunk index is very cheap in
  Swift: `0.497 ms` p95 for exact vector search.
- Query-time latency is dominated by embedding, not VectorKit search.
- The measured combined p95 is roughly `4.042 ms`, which is close to the Moss
  published `4.3 ms` p95 end-to-end number, but this still needs a single Swift
  end-to-end benchmark before treating it as an app number.
