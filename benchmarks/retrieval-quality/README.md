# Retrieval-Quality Benchmark

The versioned `v1` fixture combines human-authored workspace documents,
graded relevance judgments, exact-name and semantic queries, metadata filters,
deletion checks, replacement checks, and deterministic distractors. Document
and query embeddings are checked in so normal benchmark runs require no model
or network access.

Run the benchmark:

```bash
cargo run --release -p vectorkit-cli -- \
  bench quality \
  --fixture benchmarks/retrieval-quality/v2/fixture.json \
  --iterations 50
```

The command reports hybrid candidate quality and BM25-free vector-only F32/I8
agreement at top 5 and top 10. It exits nonzero when the default candidate pair
or vector-only I8 results fail a relevance, reference-overlap, encoding-recall,
deletion, or replacement gate.

Generate deterministic TREC artifacts without changing the production search
or wrapper APIs:

```bash
cargo run --release -p vectorkit-cli -- \
  bench quality \
  --fixture benchmarks/retrieval-quality/v2/fixture.json \
  --artifacts target/benchmarks/retrieval-quality/v2-trec \
  --iterations 1
```

The artifact directory contains normalized qrels, one TREC run per vector,
BM25, RRF, weighted-hybrid, and independent scalar exact-reference
configuration, raw Rust scores, standard metrics, duplicate-collapse counts,
and a deterministic manifest. The metrics report explicitly compares the F32
production ranking with the exact reference. TREC run scores encode rank so
external tools cannot reorder equal raw retrieval scores. Raw scores remain in
`rust-results.json`.

Install and run the independent metric validator:

```bash
python3 -m venv target/quality-eval-venv
target/quality-eval-venv/bin/pip install -r scripts/quality/requirements.txt
target/quality-eval-venv/bin/python scripts/quality/validate_trec_metrics.py \
  --artifacts target/benchmarks/retrieval-quality/v2-trec
```

An optional `--trec-eval /path/to/trec_eval` argument also checks Precision@5,
Recall@5, and MRR@10 with the official evaluator. NDCG uses VectorKit's explicit
`2^relevance - 1` gain mapping and is cross-checked with `ir_measures`.

Regenerate embeddings after intentionally editing `source.json`:

```bash
target/embedding-conversion-venv/bin/python \
  scripts/embedding/generate-retrieval-quality-fixture.py
```

V2 inherits the V1 source and adds competing documents, ambiguous queries,
graded relevance judgments, and a direct relevance-recall gate. V1 remains
checked in as the original baseline.

## V3 graph-retrieval conformance foundation

`v3/` is a small, fully synthetic, checked-in collection for the first Phase 1
graph-evaluation foundation. It implements the normative A-J population model
with separate explicit, topic-derived, and team-derived lanes. Its canonical
records exercise multi-chunk document projection and metadata override rules;
its judgments include grade-zero rows and alternative evidence sets; and its
queries cover resolver success, no-match, ambiguity, expected paths, global
exclusion, and derived-lane exclusion. Corpus and retrieval-query embeddings
are deterministic three-dimensional F32 source vectors.

Validate the collection and emit foundation-only artifacts:

```bash
cargo run -p vectorkit-cli -- \
  bench quality-v3 \
  --collection benchmarks/retrieval-quality/v3 \
  --foundation-artifacts target/v3-conformance-foundation \
  --verify-rerun
```

Independently reconstruct and check every canonical byte stream, collection
digest, population, run identity, logical-run hash, and generation fingerprint
without calling Rust:

```bash
python3 scripts/quality/validate_v3_conformance.py \
  --collection benchmarks/retrieval-quality/v3 \
  --foundation-artifacts target/v3-conformance-foundation
```

The Python command accepts `--write-fixture` only for intentional regeneration
from the frozen synthetic source model. Review the resulting collection diff
before committing it. Foundation artifacts contain validated inputs and hash
preimages only; Phase 1.1 does not fabricate A-G selections, rankings, metrics,
or timings.

Phase 1.2a qualifies the production-backed whole-corpus A-C baselines without
executing graph runs D-G. Emit a fresh partial qualification under the only
supported durable output root and prove two emissions are byte-identical:

```bash
cargo run -p vectorkit-cli -- \
  bench quality-v3 \
  --collection benchmarks/retrieval-quality/v3 \
  --qualification-artifacts target/benchmarks/v3/phase-1.2a-qualification \
  --verify-rerun
```

Then independently recalculate F32/I8/BM25/hybrid scores, filters, rankings,
document projection, TREC rows, and per-query/macro retrieval metrics:

```bash
python3 scripts/quality/validate_v3_phase_1_2a.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts target/benchmarks/v3/phase-1.2a-qualification
```

The output directory must be fresh. It is explicitly partial and
non-publication-ready, contains only A-C results, and intentionally has no final
V3 `manifest.json`. Generated artifacts stay ignored under `target/`.

Generation uses the local converted
`sentence-transformers/all-MiniLM-L6-v2` Core ML model. Review relevance
judgments independently from search output. Do not make the expected results
match whichever ranking the current implementation happens to produce.

## HotpotQA public graph adapter

Benchmark roadmap Phase 2 is complete. The frozen HotpotQA linked-abstract
adapter emits separate development and locked test V3 roots under ignored
`target/` storage. It does not commit or redistribute upstream data or generated
collections.

After acquiring the pinned sources through the documented CC BY-SA 4.0
acceptance workflow and preparing the frozen local MiniLM model, run the full
two-build qualification:

```bash
target/benchmarks/public-collections/inspection-venv/bin/python \
  scripts/quality/build_hotpotqa_graph_collection.py \
  --cache-dir target/benchmarks/public-collections \
  --abstracts-dir target/benchmarks/public-collections/sources/hotpotqa-abstracts \
  --model-dir target/embedding-models/all-MiniLM-L6-v2 \
  --output target/benchmarks/public-collections/hotpotqa-linked-abstracts-graph-v1 \
  --repeat-and-compare
```

The command verifies every source/model checksum, builds and embeds 12,670
records, independently replays the full upstream seed resolution, validates
both roots through production-backed corpus/graph ingestion, compares every
file byte-for-byte, and atomically publishes only a complete root. The
qualified adapter-manifest SHA-256 is
`8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6`.
See
`docs/product/reports/hotpotqa-graph-adapter-phase-2-report.md`
for complete identities, counts, hashes, and phase-boundary checks.

## External collections

Schema version 2 evaluation collections keep the manifest, document JSONL,
query JSONL, and TREC qrels separate. Existing schema version 1 fixtures remain
supported. An external qrels file can also override a legacy fixture with
`--qrels <qrels.tsv>`.

SciFact and NFCorpus are prepared on demand and remain under ignored `target/`
paths:

```bash
python3 scripts/quality/prepare_beir.py --dataset scifact --download-only
python3 scripts/quality/prepare_beir.py --dataset nfcorpus --download-only
python3 scripts/quality/prepare_beir.py \
  --dataset nfcorpus --split dev --download-only

target/embedding-conversion-venv/bin/python \
  scripts/quality/prepare_beir.py --dataset scifact
target/embedding-conversion-venv/bin/python \
  scripts/quality/prepare_beir.py --dataset nfcorpus

cargo run --release -p vectorkit-cli -- \
  bench quality \
  --fixture target/benchmarks/beir/scifact/vectorkit/collection.json \
  --artifacts target/benchmarks/beir/scifact/trec \
  --iterations 1
```

The preparer pins the official BEIR archive URL and checksum, validates exact
corpus, query, and qrels counts for every supported split, and creates
normalized MiniLM embeddings with the repository's local Core ML model. The
default test split is for final reporting; SciFact train and NFCorpus train/dev
are available for configuration work. Dataset files are not committed or
redistributed. Review and comply with the upstream dataset licenses and citation
requirements before publishing or redistributing benchmark artifacts.

Embedding is completed and timed by the preparer before the Rust quality run.
The Rust report records index build, persistence load, and retrieval latency as
separate measurements; none is combined with relevance metrics.
