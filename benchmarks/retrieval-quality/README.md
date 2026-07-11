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

Regenerate embeddings after intentionally editing `source.json`:

```bash
target/embedding-conversion-venv/bin/python \
  scripts/embedding/generate-retrieval-quality-fixture.py
```

V2 inherits the V1 source and adds competing documents, ambiguous queries,
graded relevance judgments, and a direct relevance-recall gate. V1 remains
checked in as the original baseline.

Generation uses the local converted
`sentence-transformers/all-MiniLM-L6-v2` Core ML model. Review relevance
judgments independently from search output. Do not make the expected results
match whichever ranking the current implementation happens to produce.
