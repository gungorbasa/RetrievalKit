# Graph Customer Fixture Contract

This directory defines the evidence required before VectorKit starts M2's
optional graph engine. It deliberately contains no customer records, graph
facts, expected results, device claims, or latency targets.

`fixture.template.json` is the handoff template. A completed sanitized fixture
must be copied to a customer-specific, versioned file and validated against
`fixture.schema.json`. The template itself is intentionally incomplete and must
not be treated as benchmark evidence.

## Required evidence

A complete fixture must contain:

- the sanitized Moss record shape and representative records;
- the current graph-construction schema plus representative sidecar data;
- the current graph engine name and exact version;
- exactly five real, high-value graph queries with deterministic expected node
  IDs and paths;
- at least one replacement/update and one deletion example with expected
  post-mutation results;
- measured record, chunk, node, edge, maximum-degree, and update-rate counts;
- every target device/OS combination;
- a reproducible latency method and required p50/p95 budgets;
- explicit equivalence rules for identity, paths, ordering, ranking/recall,
  freshness, and failures;
- a sanitization attestation saying what was removed or transformed.

## Validation expectations

Schema validation is necessary but not sufficient. Acceptance also requires:

1. Every ID referenced by an expected result or path exists in the sanitized
   records/graph input for the applicable generation.
2. Query results are reproducible and use stable external IDs, never internal
   `ChunkId` values.
3. The update and deletion cases execute from the base fixture and prove that
   superseded/deleted chunks and relationships do not survive.
4. Counts are measured from the supplied fixture or named customer corpus; they
   are not estimates presented as measurements.
5. Latency reports state device, OS, build mode, warmup, sample count,
   percentile method, concurrency, and whether embedding time is excluded.
6. The five query cases define allowed ordering differences and path/ranking
   tolerances precisely enough to produce a pass/fail result.
7. No raw secrets or personal/customer-sensitive content are committed.

Generic M2 implementation does not require this fixture. A completed fixture is
required before claiming customer-workload capacity, latency, migration
equivalence, or production cutover readiness for the bounded native engine.
