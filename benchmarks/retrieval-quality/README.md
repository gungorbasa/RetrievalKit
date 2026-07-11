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
  --fixture benchmarks/retrieval-quality/v1/fixture.json \
  --iterations 50
```

The command exits nonzero when the default candidate pair fails a relevance,
reference-overlap, encoding-recall, deletion, or replacement gate.

Regenerate embeddings after intentionally editing `source.json`:

```bash
target/embedding-conversion-venv/bin/python \
  scripts/embedding/generate-retrieval-quality-fixture.py
```

Generation uses the local converted
`sentence-transformers/all-MiniLM-L6-v2` Core ML model. Review relevance
judgments independently from search output. Do not make the expected results
match whichever ranking the current implementation happens to produce.
