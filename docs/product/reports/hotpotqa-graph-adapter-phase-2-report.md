# HotpotQA Graph Adapter Phase 2 Qualification Report

Date: 2026-07-17  
Result: PASS  
Scope: benchmark roadmap Phase 2 only

## Qualified command

Before first source download, explicit license acceptance was recorded and all
six pinned public-source artifacts were verified with:

```sh
target/benchmarks/public-collections/inspection-venv/bin/python \
  scripts/quality/inspect_public_graph_collections.py \
  --cache-dir target/benchmarks/public-collections \
  verify-sources --download --accept-hotpotqa-cc-by-sa-4.0
```

The acceptance record is checksum-pinned and required by both the builder and
independent validator. A missing or modified record fails closed.

The adapter was built twice from pinned inputs, independently validated after
each build, compared recursively byte-for-byte, and atomically published with:

```sh
target/benchmarks/public-collections/inspection-venv/bin/python \
  scripts/quality/build_hotpotqa_graph_collection.py \
  --cache-dir target/benchmarks/public-collections \
  --abstracts-dir target/benchmarks/public-collections/sources/hotpotqa-abstracts \
  --model-dir target/embedding-models/all-MiniLM-L6-v2 \
  --output target/benchmarks/public-collections/hotpotqa-linked-abstracts-graph-v1 \
  --repeat-and-compare
```

The published root was then checked again with the tightened independent
validator and the production-backed Rust ingestion path:

```sh
target/benchmarks/public-collections/inspection-venv/bin/python \
  scripts/quality/validate_hotpotqa_graph_collection.py \
  --root target/benchmarks/public-collections/hotpotqa-linked-abstracts-graph-v1 \
  --cache-dir target/benchmarks/public-collections \
  --model-dir target/embedding-models/all-MiniLM-L6-v2 \
  --production-cli target/debug/vectorkit
```

Both commands returned `status: valid`. The two complete adapter emissions,
including embeddings, inspection data, source inventory, manifests, and both
V3 roots, were byte-identical. The final adapter manifest SHA-256 is
`8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6`.

## Pinned sources

The builder and independent validator checked byte counts and SHA-256 before
parsing. The linked-abstract archive also matched publisher MD5
`01edf64cd120ecc03a2745352779514c`, 15,674 archive members, 15,517 data shards,
and archive-inventory SHA-256
`e2c7b289c1ed0c7e11faabd9ef1b37bceeea1a997e3673657bdfee053c6450cf`.

| Source identity | Bytes | SHA-256 |
|---|---:|---|
| `upstream/corpus/hotpotqa-linked-abstracts-2019-01-14` | 1,553,565,403 | `1acca1c5cc93c4890ea51091d2bad7c3ef6987aead127ab88728dc9e26555729` |
| `upstream/query/hotpotqa-distractor-train-00000-1908d6af` | 165,624,177 | `76d3bb3048a7cc73c1958107c0c5872a00d7e7d00c105b81e92f6769e7822e68` |
| `upstream/query/hotpotqa-distractor-train-00001-1908d6af` | 166,162,479 | `713661628434fbb19fff7392e2e321e4ed107e3c7c7784d0690946e5f722763f` |
| `upstream/query/hotpotqa-distractor-validation-1908d6af` | 27,452,575 | `c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6` |

The three query artifacts are separately identified as `upstream/judgment/`
inputs with the same bytes and hashes. The closed 16-row source inventory also
pins the attribution notice, adapter scenario, Unicode 15.1 normalization
policy, and every model/tokenizer file. Its SHA-256 is
`94febcf18315c161bee5140f0f28b65f8f91b9104d4b06d9b8e9c9508d8d9efb`.

## Frozen model and runtime

Model revision:
`c9745ed1d9f207416be6d2e6f8de32d1f16199bf`
(`sentence-transformers/all-MiniLM-L6-v2`).

| Model input | SHA-256 |
|---|---|
| Core ML model | `bb7f068c83217c5f4a39b4bad4aa75525847803485b46b7c226454a7d8f5e2fe` |
| Core ML weights | `84cbd97f75e18368c9ba9566bb51614f8f7d56f659c171124bf4447cc2145bde` |
| Core ML package manifest | `e016b09b0886f4716add9817fe1ba040a201681e27bae5f317a34bab30c39afa` |
| Conversion metadata | `31367d7310f9d5adcc727bf8f52bfb0bc6c6b31512fa3d83b7d5224cddf59784` |
| Tokenizer | `da0e79933b9ed51798a3ae27893d3c5fa4a201126cef75586296df9b4d2c62a0` |
| Tokenizer configuration | `872b6936be955bc3aea75ed599264d865626d68feede7e58b01e378e6332bd74` |

Runtime identity was
`compute_units=ALL;coremltools=9.0;numpy=2.4.6;python=3.11.14;transformers=5.12.0`.
Inference used batch size 1, sequence length 256, right padding,
longest-first right truncation, attention-mask mean pooling, F32 L2-normalized
384-dimensional vectors, and empty query/passage prefixes.

## Corpus and graph results

- Sample: 2,000 train plus 1,000 distractor-dev source queries; identity hash
  `aabf1aef707c1c518e58cc8b274b0a5fb6ce04db7cda9e6e7f888b6e1906301a`.
- Upstream abstract scan: 5,233,329 rows, 5,230,693 normalized unique titles,
  and 2,619 conflicting normalized titles.
- Frozen corpus: 12,670 records and 12,670 chunks.
- Directed `LinksTo` edges: 43,737.
- Missing requested aliases: 1,776; selected conflicts: 59.
- Seed outcomes: 2,763 resolved, 235 ambiguous, and 2 no-match.
- Corpus preimage SHA-256:
  `a59dd4edc535abde55d27aa8262d64b99d7a25c05754cd0724fef5216c5204c6`.
- `records.jsonl` SHA-256:
  `561ea1ca35506cc9cc0cee6ba44f0fbefe079e0e4fff5578774bbc71861afae0`.
- `graph-schema.json` SHA-256:
  `a15bef7d55b2680fd18e2f2f1e9452f3b80e659fd6a3d77587a1c5f39c7c716c`.

The production-backed builder accepted both roots with 12,670 records, 12,670
chunks, 25,340 nodes, and 69,077 total stored edges each. The total is 43,737
directed `LinksTo` edges plus 12,670 `ContainsChunk` and 12,670 inverse
`PartOf` ownership edges.

## Collection and embedding results

| Root | Queries | Qrels | Evidence | Exclusions | Population SHA-256 | `collection.json` SHA-256 |
|---|---:|---:|---:|---:|---|---|
| Development | 603 | 1,206 | 603 | 1,401 | `1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f` | `4ec8a04401149b04718f28b465809bd788a170c1089df5fe5e68e1ca991d633d` |
| Locked test/reporting | 297 | 594 | 297 | 704 | `9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010` | `496d21d1c686e2ef3bc36d9820d0cda058f4ca6b82bb029889ed62b48b084f72` |

Development exclusions are 1,397 global `not_in_frozen_corpus` plus four
derived-seed ambiguities. Test exclusions are 703 global plus one
derived-seed ambiguity. Derived populations are 599 development queries with
SHA-256 `da343545fa764b44c5382f4a16c933dded7bd613ae6e12768b5c2772c6739582`
and 296 test queries with SHA-256
`93c252bd743e4084c7c50e9f7dee970af2977967a62c5717ba8edc000101a9d8`.
Both expected-path files are exactly zero bytes.

| Frozen embedding file | Bytes | SHA-256 |
|---|---:|---|
| Shared corpus | 60,289,704 | `0dd2c67f457f8a1b075056410102966b8632d0fcf3ff136face0ce247d7653e7` |
| Development queries | 2,853,674 | `ad75e5a803158930969c30572cf11857b6f942904f48c867e137f86b2eeb9402` |
| Test queries | 1,405,441 | `81f7413fb572bbf5e9391d4d32b64a96fe5b6c8b20c3ecd931e0b26a6b55f96c` |

The two corpus embedding files, record files, and graph schema files are
byte-identical. The independent validator checked canonical shortest-round-trip
F32 tokens, dimension, finiteness, normalization, identity order, and hashes.

## Safety and phase boundary

The local source-acquisition workflow requires explicit CC BY-SA 4.0
acceptance before download. The generated source inventory preserves a compact
notice attributing the HotpotQA authors and Wikipedia contributors, links the
license, identifies VectorKit transformations, and states ShareAlike. No raw
archive, Parquet shard, extracted abstract, model package, tokenizer,
embedding, or generated collection is tracked by Git; all remain under ignored
`target/` paths.

The label-blind corpus was frozen before judgment eligibility was parsed. The
locked reporting query list and hashes were validated, but no retrieval result
from that split was produced or inspected for tuning. Phase 3 retrieval runs,
configuration tuning, device benchmarks, and marketing work did not begin.

Phase 2 is complete. Phase 3 remains inactive until deliberately started. The
next benchmark task is the Phase 3 A-G quality ablation: select configuration
using development data only, freeze it, and then run the locked reporting split
without per-query tuning or qrels leakage.
