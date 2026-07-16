# Graph Retrieval Evaluation Contract V3

Status: Phase 0 complete; approved implementation contract; Phase 1 active

Date: 2026-07-15

Last revised: 2026-07-16

This document is the normative Phase 0 contract for graph-aware retrieval
evaluation. It refines the Phase 0 section of
`docs/product/complete-retrieval-benchmark-and-marketing-roadmap.md`. The
repository owner approved both documents and selected the conservative device
on 2026-07-15. The first independent review found blocking underspecified
policies; see
`docs/product/reports/graph-retrieval-phase-0-independent-review.md`. The third
focused revision passed two fresh isolated implementation-author reviews on
2026-07-16 under section 12. Phase 1 is authorized.

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, and **MAY** are
normative. “Document” in evaluation formulas means a canonical record and is
identified by its stable `record_id`. Chunks are retrieval units owned by that
record.

## 1. Scope and architecture boundary

V3 evaluates three separate capabilities composed over one canonical corpus:

```text
CorpusIndex
  + RetrievalIndex                   whole-corpus retrieval
  + GraphEngine                      graph selection
  + GraphEngine then RetrievalIndex  graph-scoped retrieval
```

The collection format MUST preserve the production architecture:

- `records.jsonl` owns canonical records, chunks, text, fields, and metadata.
- `graph-schema.json` configures graph construction from canonical record
  fields. It MUST NOT contain retrieval dimensions, vector encodings,
  embeddings, similarity metrics, fusion parameters, or ranking limits.
- Corpus and query embeddings live in separate files. They are inputs to the
  retrieval capability, not fields on canonical chunks or graph nodes.
- A graph selection is an immutable, generation-bound candidate scope. It is
  produced before ranking and is not a second corpus or a relevance label.
- Benchmark manifests and diagnostics MAY describe the composed workflow, but
  MUST NOT represent `GraphEngine` and `RetrievalIndex` as one engine.

This contract does not add production APIs, automatic entity extraction, a
general graph database, ANN, a dataset adapter, or a device harness.

## 2. Common representation rules

### 2.1 Text and identifiers

All non-empty text files MUST be UTF-8 without a byte-order mark, use LF line
endings, and end in exactly one LF. A row-oriented `.jsonl`, `.trec`, or `.tsv`
file whose required population has zero rows is the sole exception: it is
exactly zero bytes, not one blank line. A `.json` file is never empty. Input text is
preserved byte-for-byte after UTF-8 validation; preprocessing changes belong
in a new collection version.

`collection_id`, `collection_version`, `corpus_id`, `query_id`, `record_id`,
and `chunk_key` MUST:

- contain 1 through 128 ASCII characters;
- match `[A-Za-z0-9][A-Za-z0-9._:-]*`;
- be case-sensitive; and
- remain stable across rebuilds of the same collection version.

The pair `(record_id, chunk_key)` is the stable chunk identity. A `chunk_key`
need only be unique within its record. Internal dense `ChunkId`, graph node
ordinals, ingestion order, and generation IDs are never collection identities.

Graph `node_type` and `relationship_type` values MUST obey the production graph
identifier rules and be used with exact case. A canonical node identity is the
structured tuple:

```text
(node_type, source_kind, record_id[, chunk_key])
```

It MUST NOT be flattened into a delimiter-based string for comparison.

Its exact JSON representation is one of:

```text
{"node_type":"Topic","source":{"kind":"record","record_id":"alpha"}}
{"node_type":"Chunk","source":{"kind":"chunk","record_id":"alpha","chunk_key":"summary"}}
```

`record_type` and every segment of a field path MUST match the current
production identifier grammar `[A-Za-z_][A-Za-z0-9_]{0,63}`. The stricter
evaluation-ID grammar above is intentional: `query_id` and `record_id` also
appear as whitespace-delimited TREC tokens.

### 2.2 Canonical JSON and JSON Lines

Every `.json` file and every object in a `.jsonl` file MUST use the following
canonical encoding:

- object keys in ascending Unicode code-point order;
- compact `,` and `:` separators with no insignificant whitespace;
- arrays in the semantic orders defined below, never object iteration order;
- strings escaped by the exact rules below;
- integers in base 10 with no leading zero or plus sign;
- finite floating-point values only;
- `-0` serialized as `0`; and
- the following canonical shortest-round-trip decimal representation of the
  source IEEE-754 value (`f32` for native raw scores and embeddings, `f64` for
  calculated metrics and seconds).

The floating-point algorithm is the Ryu shortest-round-trip rule with
round-to-nearest, ties-to-even parsing. Among equal-length candidates choose
the decimal closest to the exact binary value; if still tied choose the one
whose last significand digit is even. Use plain notation when the normalized
base-10 exponent is from `-6` through `20`, inclusive, and scientific notation
otherwise. Scientific notation has one digit before the decimal point,
lowercase `e`, no `+`, and no leading zero in the exponent. In either notation
remove a trailing decimal point and all insignificant trailing fractional
zeros. Both positive and negative zero serialize as `0`. These rules, rather
than a host language's default number formatter, define the bytes.

NaN, positive infinity, and negative infinity are invalid. A JSONL file has
one canonical object followed by LF per row and no blank rows. Unknown fields
are errors unless a later schema version explicitly introduces them.

String escaping is byte-deterministic. Emit `\"` for quotation mark, `\\` for
reverse solidus, and the two-character escapes `\b`, `\t`, `\n`, `\f`, and
`\r` for U+0008, U+0009, U+000A, U+000C, and U+000D respectively. Emit every
other U+0000 through U+001F code point as `\u00xx` with lowercase hexadecimal
digits. Do not escape solidus, non-ASCII characters, U+2028, or U+2029. Emit
all other Unicode scalar values directly as UTF-8. Lone surrogates are invalid
Unicode input and MUST be rejected. These rules choose exactly one spelling
for every valid string.

### 2.3 Ordering and hashes

“Lexical” means unsigned UTF-8 byte order. Required input order is:

- records by `record_id`;
- chunks within a record by `chunk_key`;
- corpus embeddings by `(record_id, chunk_key)`;
- queries and query embeddings by `query_id`;
- qrels by `(query_id, record_id)`;
- evidence rows by `query_id`, expected-path rows by
  `(query_id, seed_policy)`, and exclusion rows by
  `(query_id, lane, phase, reason, source)`; and
- set-valued identifier arrays after duplicate rejection, in lexical order.

Every manifest path is relative, uses `/`, contains no `.` or `..` component,
and resolves beneath the collection or artifact root. Every referenced file
MUST have a `sha256` digest recorded as 64 lowercase hexadecimal characters.
Unless a field states otherwise, a query-population hash is SHA-256 over the
UTF-8 bytes of its lexically ordered query IDs, each followed by one LF. A
dirty-source hash is SHA-256 over a canonical JSON array of
`{"path":<repository-relative path>,"sha256":<digest>}` objects sorted by
path. Its file set is exactly the regular files returned at repository root by
`git ls-files --cached --others --exclude-standard -z`, with paths interpreted
as UTF-8 and rejected if invalid. A symlink or non-regular returned path is an
error. The artifact output root MUST be outside this set; an output root inside
the repository must therefore be ignored before execution. This closed
definition includes all tracked and non-ignored untracked source state and
excludes ignored build and artifact outputs without an implementation-specific
notion of which inputs were “consumed.”

## 3. V3 evaluation collection

### 3.1 Required layout

```text
<collection-root>/
  collection.json
  records.jsonl
  graph-schema.json
  queries.jsonl
  corpus-embeddings.f32.jsonl
  query-embeddings.f32.jsonl
  qrels.tsv
  evidence-judgments.jsonl
  expected-paths.jsonl
  exclusions.jsonl
  manifests/
    preprocessing.json
    chunking.json
    embedding.json
    graph-construction.json
    seed-policy.json
    split.json
```

All files are required, even when a JSONL file has zero rows. V3 intentionally
extends the V2 pattern of a small manifest plus separate corpus, query, and
qrels files. Raw upstream data and downloaded datasets remain outside the
repository. No other regular file or directory is permitted beneath the
collection root.

### 3.2 `collection.json`

`collection.json` MUST contain exactly:

| Field | Type | Requirement |
| --- | --- | --- |
| `schema_version` | integer | Exactly `3`. |
| `collection_id` | identifier | Logical collection identity. |
| `collection_version` | identifier | Immutable version; changing any semantic input requires a new value. |
| `corpus_id` | identifier | Identity of the frozen canonical corpus. |
| `split` | string | `development` or `test`. A mixed split is invalid. |
| `top_k` | integer | Positive; canonical public cutoff, normally `10`. |
| `evaluation_depth` | integer | At least `10` and at least `top_k`; maximum emitted document depth. |
| `relevance_threshold` | integer | Exactly `1` for V3. |
| `paths` | object | Exact relative paths for every file in section 3.1. |
| `counts` | object | Exact record, chunk, query, qrel-row, evidence-row, expected-path-row, and exclusion-row counts. |
| `files` | array | Objects `{path, bytes, sha256}`, sorted by `path`; excludes `collection.json` itself. |

`paths` contains exactly these keys and values:

```json
{"chunking_manifest":"manifests/chunking.json","corpus_embeddings_f32":"corpus-embeddings.f32.jsonl","embedding_manifest":"manifests/embedding.json","evidence_judgments":"evidence-judgments.jsonl","exclusions":"exclusions.jsonl","expected_paths":"expected-paths.jsonl","graph_construction_manifest":"manifests/graph-construction.json","graph_schema":"graph-schema.json","preprocessing_manifest":"manifests/preprocessing.json","qrels":"qrels.tsv","queries":"queries.jsonl","query_embeddings_f32":"query-embeddings.f32.jsonl","records":"records.jsonl","seed_policy_manifest":"manifests/seed-policy.json","split_manifest":"manifests/split.json"}
```

`counts` contains exactly the non-negative integer fields `records`, `chunks`,
`queries`, `qrel_rows`, `evidence_rows`, `expected_path_rows`, and
`exclusion_rows`. Each `files` entry contains exactly `path`, non-negative
integer `bytes`, and `sha256`. Its path set MUST equal the values of `paths`;
there are no unhashed collection inputs.

No timestamp is permitted. `collection_id + collection_version` identifies an
immutable logical collection; `corpus_id` MUST be equal between development
and test collections that share the same frozen corpus.

### 3.3 Canonical records and chunks: `records.jsonl`

Each row is:

```json
{"chunks":[{"chunk_key":"summary","metadata":{"tenant":{"type":"string","value":"red"}},"text":"..."}],"content":null,"fields":{"title":{"type":"string","value":"Alpha"}},"metadata":{},"record_id":"alpha","record_type":"Topic"}
```

Required record fields are `record_id`, `record_type`, `content`, `fields`,
`metadata`, and `chunks`. `content` is a string or null and preserves the
optional canonical record payload; it is not implicitly concatenated into
chunk text. `chunks` MUST be non-empty. Required chunk fields are
`chunk_key`, `text`, and `metadata`. Empty chunk text is invalid.

`fields` maps field names to tagged values that map one-to-one to production
`RecordValue` variants. Lists and objects recursively contain tagged values:

```text
{"type":"null"}
{"type":"boolean","value":<boolean>}
{"type":"integer","value":<signed 64-bit integer>}
{"type":"float","value":<finite f64>}
{"type":"string","value":<string>}
{"type":"list","value":[<tagged value>, ...]}
{"type":"object","value":{<field name>:<tagged value>, ...}}
```

Explicit graph references are a tagged string or a tagged list containing only
tagged strings whose values are stable target `record_id` values. The graph
schema, not the value alone, declares that a field is a reference. Metadata is
a flat object whose values are exactly one of:

```text
{"type":"string","value":<string>}
{"type":"integer","value":<signed 64-bit integer>}
{"type":"float","value":<finite f64>}
{"type":"boolean","value":<boolean>}
{"type":"timestamp_millis","value":<signed 64-bit integer>}
```

Null, lists, and objects are invalid metadata values. Chunk metadata overrides
record metadata on the same key, matching the canonical corpus contract.

One `record_id` is one evaluation document. Multiple chunks from that record
therefore project to the same document for qrels-based metrics.

### 3.4 Graph input: `graph-schema.json`

This file is the canonical production `GraphSchema` input expressed with
snake-case enum values. It contains exactly `version`, `record_nodes`,
`relationships`, and `chunk_nodes`.

Each record-node rule contains `record_type`, `node_type`, and
`queryable_fields`, where a field path is a non-empty array of field-name
segments. Each relationship rule contains:

```text
relationship_type
source_node_type
target_node_type
source_field
cardinality              one | optional_one | many
missing_target           error | omit_edge
duplicate_references     error | deduplicate
allow_self_edge          boolean
inverse_relationship     relationship type or null
```

`chunk_nodes` is null or contains `node_type`, `owns_relationship`, and
`inverse_relationship`. Nodes and edges are derived only from `records.jsonl`
and this schema. A record-node source is `{kind:"record", record_id}`; a
chunk-node source is `{kind:"chunk", record_id, chunk_key}`. A canonical path
edge is:

```text
relationship_type, direction, source_node, target_node, occurrence_ordinal
```

`source_node` and `target_node` always use the stored relationship's canonical
orientation, matching production `EdgeId`; they are not swapped for an
incoming traversal. `direction` records whether the query traversed that edge
outgoing or incoming. Consequently, an outgoing hop moves from `source_node`
to `target_node`, while an incoming hop moves from `target_node` to
`source_node`. Self-edge direction comes from the executing traversal step.
`occurrence_ordinal` is the production edge occurrence ordinal after the
schema's duplicate-reference policy is applied.

Relationship order in the file is semantic and MUST be frozen. The graph
construction manifest records upstream derivation, missing-target behavior,
and the canonical schema hash. Qrels, evidence, and expected paths MUST NOT be
inputs to this file or its construction.

### 3.5 Embeddings

`corpus-embeddings.f32.jsonl` has exactly one row for every chunk:

```json
{"chunk_key":"summary","record_id":"alpha","values":[0.1,-0.2]}
```

`query-embeddings.f32.jsonl` has exactly one row for every query whose `tasks`
contains `retrieval`, including queries excluded only from a derived seed lane:

```json
{"query_id":"q1","values":[0.1,-0.2]}
```

Both files use finite `f32` values and the dimension declared in
`manifests/embedding.json`. Missing and unexpected embedding keys are errors.
The values are raw model-output F32 vectors before VectorKit's metric-specific
runtime normalization. The same source vectors feed all canonical runs. I8
runs first apply the section 3.8 normalization policy and then quantize through
the frozen VectorKit configuration; an adapter MUST NOT use a different
embedding model for I8. Embeddings MUST NOT appear in
`records.jsonl`, `queries.jsonl`, or `graph-schema.json`.

### 3.6 Queries: `queries.jsonl`

Each query row contains exactly:

| Field | Type | Requirement |
| --- | --- | --- |
| `query_id` | identifier | Unique and stable. |
| `split` | string | Equal to `collection.json.split`. |
| `category` | string | Non-empty diagnostic slice. |
| `text` | string | Non-empty retrieval text. |
| `metadata_filter` | object or null | Canonical typed filter AST described below. |
| `explicit_seed` | object or null | Application-provided structured seed. |
| `derived_seed_policy_id` | identifier or null | Policy in `manifests/seed-policy.json`. |
| `traversal` | object | `steps` and `limits`, even for zero-step selection. |
| `tasks` | array | Non-empty, duplicate-free, lexically sorted subset of `retrieval`, `evidence`, `path`. |

A seed is exactly one of:

```text
{"kind":"node_ids","nodes":[canonical node identity, ...]}
{"kind":"equals","node_type":...,"field":[...],"values":[tagged graph scalar, ...]}
```

Node and value arrays are non-empty, duplicate-free, and lexical after their
canonical JSON encoding. Graph scalars are string, signed 64-bit integer, or
boolean, matching the production seed surface.

For a query whose `tasks` contains `evidence` or `path`, at least one of
`explicit_seed` and `derived_seed_policy_id` MUST be non-null. Both MAY be
non-null; that creates two independently evaluated seed lanes, not a fused
seed. A retrieval-only query MAY set both to null.

`tasks` declares metric applicability, not seed-lane membership. `retrieval`
makes the query eligible for A-C and, when a seed lane is present and resolves,
E-G. `evidence` requires exactly one evidence-judgment row and enables final
and candidate evidence metrics wherever the run supplies the needed result.
`path` permits expected-path rows and enables Path Accuracy in a seed lane only
when a row for that lane exists. A query without `evidence` MUST NOT have an
evidence-judgment row, and a query without `path` MUST NOT have an
expected-path row. A retrieval-only query with a seed is valid and participates
in that seed lane; its evidence and path metrics are `not_applicable`.

`traversal.steps` is an ordered array of objects containing
`relationship_type`, `direction` (`outgoing` or `incoming`), `min_hops`, and
`max_hops`. `0 <= min_hops <= max_hops`. `traversal.limits` contains positive
`max_hops`, `max_visited`, `max_results`, and `max_working_bytes`, within the
production hard limits. Steps, hop bounds, and limits are query inputs frozen
before test execution; they are never inferred from expected paths.

The metadata-filter AST is one of `equals`, `not_equals`, `in`, `range`,
`exists`, `all`, or `any`, with typed scalar operands. These map one-to-one to
the current production `Filter` variants. Children of commutative `all` and
`any` nodes are sorted by canonical JSON bytes after
duplicate rejection. The same filter AST is used in runs A through G. Graph
selection itself does not absorb retrieval metadata filtering: projection and
filter intersection are separately recorded.

The exact AST variants are:

```text
{"op":"equals","field":"tenant","value":<tagged metadata scalar>}
{"op":"not_equals","field":"tenant","value":<tagged metadata scalar>}
{"op":"in","field":"tenant","values":[<tagged metadata scalar>, ...]}
{"op":"range","field":"created_ms","lower":<tagged numeric scalar or null>,
 "upper":<tagged numeric scalar or null>}
{"op":"exists","field":"tenant"}
{"op":"all","children":[<filter>, ...]}
{"op":"any","children":[<filter>, ...]}
```

`field` is a non-empty flat metadata key. `in` and logical child arrays are
non-empty. `in` values are type-homogeneous, duplicate-free, and sorted by
canonical bytes. `range` has at least one non-null bound; its bounds have the
same integer, float, or timestamp type, and both bounds are inclusive. Logical
depth is at most 16. General negation and exclusive range bounds are invalid
because they are not production capabilities.

### 3.7 Qrels, evidence, paths, splits, and exclusions

`qrels.tsv` uses four whitespace-separated TREC fields:

```text
query_id 0 record_id relevance
```

Relevance is a canonical unsigned base-10 integer from `0` through `127`, with
no leading zero unless it is `0`; `>= 1` is relevant. Duplicate
`(query_id, record_id)` rows are invalid. A query whose tasks contain
`retrieval` MUST have at least one positive judgment; a query without that task
MUST have no qrel row. The second field is exactly `0`. Fields are separated by
exactly one ASCII space. Blank lines, comments, leading/trailing whitespace,
and additional fields are invalid; rows are ordered as required by section 2.3.

`evidence-judgments.jsonl` has exactly one row per query whose `tasks` contains
`evidence` and no row for any other query:

```json
{"evidence_sets":[["d1","d2"],["d1","d3"]],"query_id":"q1"}
```

`evidence_sets` is non-empty. Each inner array is one complete valid
alternative set of required supporting documents. Sets are non-empty,
internally lexical, duplicate-free, unique, and sorted by canonical JSON bytes.
Every referenced document MUST exist in the frozen corpus. Multiple sets
express alternatives, not documents that may be pooled into one easier set.

`expected-paths.jsonl` is optional in content but required as a file. It has
at most one row per `(query_id, seed_policy)`, where `seed_policy` is
`explicit` or the query's derived policy ID:

```json
{"expected_paths":[[{"direction":"outgoing","occurrence_ordinal":0,"relationship_type":"owns","source_node":{"node_type":"Team","source":{"kind":"record","record_id":"mobile"}},"target_node":{"node_type":"Product","source":{"kind":"record","record_id":"phone"}}}]],"query_id":"q1","seed_policy":"explicit"}
```

`expected_paths` is non-empty. Each member is one complete acceptable ordered
path; an individual path MAY be empty to judge a zero-step match. Node
identities are structured as in section 3.4. Paths are unique and sorted by
canonical JSON bytes. A row is valid only when the query declares `path` and
supplies the named seed lane. Expected paths may be absent for a path query or
for one of its lanes; Path Accuracy in that lane is then `not_applicable`.

For qrels and evidence, a document satisfies a query's metadata filter when at
least one active chunk of that document satisfies the filter after chunk
metadata overrides record metadata. Every positively judged or supporting
document MUST satisfy that rule. A document whose active chunks disagree is
not a conflict merely because some chunks fail; `filter_label_conflict` means
that none satisfies.

`exclusions.jsonl` has rows:

```json
{"details":"...","lane":"global","phase":"pre_freeze","query_id":"q1","reason":"missing_complete_evidence","source":"adapter"}
```

Allowed reasons are `not_in_frozen_corpus`, `missing_complete_evidence`,
`invalid_upstream_record`, `duplicate_identity`, `filter_label_conflict`,
`no_relevant_documents`, `derived_seed_no_match`, and
`derived_seed_ambiguous`. Every row contains exactly `details`, `lane`,
`phase`, `query_id`, `reason`, and `source`; `phase` is `pre_freeze`.

For the first six reasons, `lane` is exactly `global`. A globally excluded ID
MUST NOT appear in `queries.jsonl`, query embeddings, qrels, evidence, or
expected paths; it exists in the frozen collection only in `exclusions.jsonl`
and exclusion counts. For `derived_seed_no_match` and
`derived_seed_ambiguous`, `lane` is exactly the query's
`derived_seed_policy_id`, the query MUST remain in `queries.jsonl`, and the
exclusion applies only to that derived lane. Such a query remains eligible for
A-C and for an independent explicit lane. There is exactly one exclusion row
per excluded `(query_id, lane)` and no `explicit` lane exclusion: an invalid
explicit seed is a collection construction error under section 6.1. The file
MUST NOT change after test configuration is frozen.

`manifests/split.json` records upstream release and split identifiers, archive
URL and checksum, license identifier and notice source ID, deterministic corpus
selection rule, counts before and after each exclusion reason, and the SHA-256
of the final ordered `queries.jsonl` query-ID list. That final list is after
global exclusions and before derived-lane exclusions, so it includes derived
resolution failures. The declared, failed, and successful lane hashes are in
`seed-policy.json`, not duplicated here. Development and test query lists are
disjoint.

### 3.8 Transformation manifests

Every transformation manifest contains exactly `inputs`, `outputs`,
`parameters`, `policy_id`, `policy_version`, `schema_version`, and `tool`.
`schema_version` is integer `1`;
policy and version are identifiers; `tool` is exactly
`{"name":<non-empty string>,"version":<non-empty immutable version>}`. Inputs
are arrays of exactly `{"sha256":<sha256>,"source_id":<non-empty string>}`
sorted by `source_id` with no duplicate ID. Outputs are arrays of exactly
`{"path":<path>,"sha256":<sha256>}`, sorted by path with no duplicate path.
Inputs contain every file or byte stream read by the stage; a collection input
uses source ID `collection/<section 2.3 path>`, while an external upstream
input uses a stable adapter-defined ID beginning with exactly one of
`upstream/corpus/`, `upstream/graph/`, `upstream/judgment/`,
`upstream/license/`, `upstream/model/`, `upstream/query/`,
`upstream/scenario/`, or `upstream/tokenizer/`. Outputs contain
every semantic file written beneath the collection root except the manifest
itself. That sole exclusion prevents a self-referential digest. A stage MUST
reject a supplied but unlisted input. No manifest contains a timestamp or an
unknown field.

The stage DAG and file ownership are exact. A manifest MUST have precisely the
collection inputs, permitted upstream-prefix inputs, and outputs in this table;
an empty output array is the canonical `[]`. There are no materialized
intermediates and no stage overwrites another stage's output.

| Manifest | Exact collection inputs | Permitted upstream prefixes | Exact outputs |
| --- | --- | --- | --- |
| `preprocessing.json` | none | `upstream/corpus/` | none |
| `chunking.json` | none | `upstream/corpus/` | `records.jsonl` |
| `graph-construction.json` | `records.jsonl` | `upstream/graph/` | `graph-schema.json` |
| `split.json` | `graph-schema.json`, `records.jsonl` | `upstream/judgment/`, `upstream/license/`, `upstream/query/`, `upstream/scenario/` | `evidence-judgments.jsonl`, `exclusions.jsonl`, `expected-paths.jsonl`, `qrels.tsv`, `queries.jsonl` |
| `seed-policy.json` | `exclusions.jsonl`, `graph-schema.json`, `queries.jsonl`, `records.jsonl` | `upstream/scenario/` | none |
| `embedding.json` | `queries.jsonl`, `records.jsonl` | `upstream/model/`, `upstream/tokenizer/` | `corpus-embeddings.f32.jsonl`, `query-embeddings.f32.jsonl` |

Collection-input entries use the listed `collection/<path>` source ID and the
digest of the final file. Each permitted upstream prefix MUST have at least one
entry except `upstream/graph/` and `upstream/scenario/`, which MAY be absent
when records alone define the graph or no application scenario is used. For
each permitted prefix present in the union source inventory, the stage MUST
list every inventory entry having that prefix, never an
implementation-selected subset.
Preprocessing is a logical policy stage; chunking applies its frozen parameters
in memory and alone owns the final canonical records file. Split applies the
already frozen seed-policy parameters in memory; the seed-policy manifest is
then emitted from final queries/exclusions and owns no second exclusions file.
`collection.json` is assembled last and is not a transformation-stage output.

The exact `parameters` objects are:

- `preprocessing.json`: `field_selection` (a non-empty array of unique field
  paths sorted by canonical bytes), `source_record_id_path` (field path),
  `source_record_type_path` (field path or null), `source_to_record_mapping`
  (non-empty frozen string), `text_join_separator` (string), `title_path`
  (field path or null), `unicode_handling` (non-empty frozen string), and
  `whitespace_rules` (non-empty frozen string).
- `chunking.json`: `boundary_policy`, `chunker_name`, `chunker_version`,
  `source_offset_policy`, `stable_key_derivation`, and `units` (non-empty
  frozen strings), plus positive integer `maximum_size` and non-negative
  integer `overlap` smaller than `maximum_size`.
- `embedding.json`: `dimension` and `sequence_length` (positive integers),
  `document_prefix`, `input_construction`, `model_checksum`, `model_id`,
  `model_output_normalization`, `model_revision`, `pooling`, `query_prefix`, `runtime`,
  `tokenizer_id`, `tokenizer_revision`, and `truncation_policy` (strings, with
  IDs/revisions/checksum non-empty), and `quantization` (the exact object
  below).
- `graph-construction.json`: `duplicate_references`, `missing_target`,
  `node_derivation`, `relationship_derivation` (non-empty frozen strings),
  `inverse_edges` and `self_edges` (booleans), `judgment_inputs_sha256`
  (exactly null), `schema_sha256` (SHA-256), and `source_fields` (a
  duplicate-free canonical-byte-sorted array of field paths).

The embedding quantization object is exactly:

```json
{"arithmetic":"ieee754_f32_each_operation","clamp_max":127,"clamp_min":-128,"dot_accumulator":"signed_i32_exact","encoding_expression":"value_times_reciprocal_scale","kind":"symmetric_per_vector_i8","rounding":"half_away_from_zero","scale_divisor":127,"score_expression":"f32_i32_dot_times_query_scale_times_chunk_scale","zero_vector_scale":0}
```

The retrieval normalization policy is exactly one of these objects:

```text
{"arithmetic":"ieee754_f32_each_operation","input":"source_f32","inverse_norm":"sqrt_then_reciprocal","kind":"unit_l2_before_encoding","reduction":"index_order_left_to_right","sqrt":"correctly_rounded_f32","zero_vector":"unchanged"}
{"arithmetic":"none","input":"source_f32","kind":"none"}
```

For `unit_l2_before_encoding`, start `squared_norm` at positive F32 zero and,
in increasing vector-index order, round `value * value` to F32 and then round
`squared_norm + product` to F32. If the result is exactly zero, leave the vector
unchanged. Otherwise take the correctly rounded F32 square root, its F32
reciprocal, and in index order replace each value with the F32-rounded product
`value * inverse_norm`. The normalized F32 vector is then passed to F32 storage
or to the I8 quantizer. `none` passes source F32 values directly. No fused
operation, wider accumulator, reassociation, or second normalization is
permitted.

The canonical weighted-hybrid BM25 policy is exactly:

```text
{"b":0.75,"k1":1.2,"lowercase":"rust_str_to_lowercase","stop_words":[],
 "tokenizer_id":"unicode-segmentation-unicode_words",
 "tokenizer_library_sha256":"c6f5d3c3b1bf09027a88a6bc961fc00497d651009560b5463668dc81b0fa87a8",
 "tokenizer_version":"1.13.3",
 "unicode_lowercase_tables_sha256":<sha256>,
 "unicode_version":<"major.minor.patch" decimal string>}
```

The Unicode-table digest hashes the compact canonical JSON array of every
non-identity lowercase mapping supported by the runtime, sorted by input scalar
value, with each member exactly `{"from":<Unicode scalar integer>,"to":[<output scalar integers>...]}`.
This semantic preimage avoids hashing an implementation-specific compressed
table layout. Query and chunk text are segmented by the pinned
`unicode_words` implementation, each token is mapped by the pinned Unicode
lowercase tables, empty tokens and exact stop-word members are removed, and
the current V3 stop-word set is empty. C and G MUST use the same complete
policy object. Unicode version components have no leading zeros unless the
component is exactly `0`.

The embedding dimension MUST satisfy `dimension * 16384 <= 2147483647`, so
the signed-I32 I8 dot accumulator cannot overflow.

For each vector, production I8 encoding computes `max_abs` in `f32`. If it is
zero, every encoded value is zero and scale is `0`. Otherwise scale is the
`f32` result `max_abs / 127`, and `inverse_scale` is the `f32` reciprocal of
scale. Each value is the `f32` multiplication `value * inverse_scale`, rounded
halfway away from zero, clamped to `[-128,127]`, and cast to signed I8. B, C,
F, and G score with an exact signed-I32 dot product converted to `f32`, then
left-to-right `f32` multiplication by query scale and chunk scale. They MUST use
this object and algorithm; A, D, and E do not quantize.

The `parameters` object in `seed-policy.json` contains exactly
`derived_policies`, `explicit_policy`, and `normalization`. `normalization` is
exactly:

```text
{"case_folding":"unicode_default_full_case_folding",
 "normalization_form":"NFC",
 "normalization_version":"unicode-15.1-nfc-full-fold-whitespace-v1",
 "punctuation":"preserve","unicode_tables_sha256":<sha256>,
 "unicode_version":"15.1",
 "whitespace":"unicode_white_space_to_ascii_collapse_trim"}
```

The checksum pins the exact Unicode tables or library artifact. `explicit_policy` contains exactly
`policy_id`, `policy_version`, and `provenance`. Its ID is `explicit`, and
`provenance` has one row per query in `X_exp`, sorted by query ID, each exactly
`{"query_id":<ID>,"source_id":<non-empty string>,"transformation_id":<non-empty string>}`.

`derived_policies` is sorted by policy ID and contains one object per derived
policy, with policy IDs non-empty and unique. A derived ID MUST NOT be
`explicit`, `global`, `na`, or `none`; these values are reserved by lane,
exclusion, and run-ID serialization. Every non-null query
`derived_seed_policy_id` MUST resolve to exactly one object. Each object contains
exactly `aliases`, `alias_table_sha256`,
`declared_population_sha256`, `failure_population_sha256`, `policy_id`,
`policy_version`, `source_fields`, and `successful_population_sha256`.
`source_fields` is a non-empty duplicate-free canonical-byte-sorted array of
field paths. `aliases` is the complete frozen alias table. Each alias row is
exactly:

```json
{"alias":<non-empty string>,"normalized_alias":<non-empty string>,"seed":<canonical seed>,"source":{"field":<field path>,"record_id":<record ID>}}
```

Only string values at the declared source fields whose section 6.2 normalized
form is non-empty create alias rows. Empty strings, strings that normalize to
empty, null, numeric, boolean, list, and object values create none. The
`normalized_alias` MUST equal that normalized form. Exact
duplicate rows are rejected. Distinct provenance rows and distinct seeds MAY
share an alias and are retained so ambiguity is observable. Rows sort by
`(normalized_alias UTF-8 bytes, canonical seed bytes, record_id, canonical
field-path bytes, alias UTF-8 bytes)`. `alias_table_sha256` hashes the compact
canonical encoding of the entire `aliases` array. Population hashes are the
section 5.1 `X_p`, `F_p`, and `S_p` hashes.

The `parameters` object in `split.json` contains exactly `archive_sha256`,
`archive_url`, `collection_rule`, `development_population_sha256`,
`exclusion_counts`, `license_id`, `license_notice_source_id`, `release_id`,
`source_inventory_sha256`, `split_id`, `test_lock_sha256`, and
`test_population_sha256`.
Strings are non-empty; digests are SHA-256. `exclusion_counts` has one
`{"after":<integer>,"before":<integer>,"excluded":<integer>,"lane":<lane>,"reason":<section 3.7 reason>}`
row for every applicable `(lane, reason)`, sorted by `(lane, reason)`. The
first six reasons use lane `global`; each derived failure reason has one row
per derived policy. Counts are query counts in that lane immediately before
and after that reason's lexically ordered stage, and `after = before -
excluded`. Development and test population hashes use their complete ordered
`queries.jsonl` IDs and their sets are disjoint.
`source_inventory_sha256` hashes the compact canonical array formed from the
union of every `upstream/` input entry in all six manifests, keyed and sorted
by `source_id`. Repeated identical entries collapse to one; the same source ID
with different digests is invalid.
`license_notice_source_id` MUST name exactly one `upstream/license/` entry in
that inventory.
`test_lock_sha256` hashes the compact canonical object
`{"collection_rule":<same value>,"development_population_sha256":<same value>,"exclusion_counts":<same array>,"release_id":<same value>,"source_inventory_sha256":<same value>,"split_id":<same value>,"test_population_sha256":<same value>}`.

Any manifest change creates a new `collection_version`.

## 4. Deterministic evaluation artifacts

### 4.1 Required layout

```text
<artifact-root>/
  qrels.tsv
  evidence-judgments.jsonl
  expected-paths.jsonl
  exclusions.jsonl
  runs/
    <run-id>.trec
  graph-selections/
    <selection-run-id>.jsonl
  graph-paths/
    <selection-run-id>.jsonl
  rust-results.json
  metrics.json
  timing-samples.jsonl
  manifest.json
```

No other regular file or directory is permitted beneath the artifact root.

`runs/` contains one TREC file for A, B, and C and one for every E, F, and G
seed-lane run defined by section 5.1. D has no ranked retrieval output.
`graph-selections/` and `graph-paths/` contain every D seed-lane run plus the
selections independently re-executed for every E, F, and G seed-lane run.
Logical selection and path contents of valid executions MUST agree across
D/E/F/G for the same query and seed lane; corpus generation IDs may differ and
are diagnostic only.
`selection-run-id` is the corresponding stable D, E, F, or G run ID from
section 4.2; there is no second hidden identifier namespace.

`qrels.tsv`, `evidence-judgments.jsonl`, `expected-paths.jsonl`, and
`exclusions.jsonl` are byte-identical copies of the already canonical
collection inputs. This keeps V3 artifacts compatible with the existing TREC
validator while giving graph metrics separate judgments.

### 4.2 Run IDs

A run configuration is the following exact canonical JSON object; it has no
other fields:

```text
{"bm25_policy":<section 3.8 object or null>,
 "candidate_limits":{"keyword":<positive integer or null>,"vector":<positive integer or null>},
 "collection_id":<identifier>,"collection_version":<identifier>,
 "corpus_id":<identifier>,"evaluation_depth":<positive integer>,
 "fusion_alpha":<finite f32 or null>,"graph_schema_sha256":<sha256 or null>,
 "implementation_revision":{"binary_sha256":<sha256>,"git_commit":<40 lowercase hex>,
                            "source_sha256":<sha256 or null>},
 "metadata_filter_policy_id":"v3-query-filter-ast-v1",
 "metric":<"cosine" | "dot_product" | null>,
 "normalization":<"unit_l2" | "none" | null>,
 "normalization_policy":<section 3.8 object or null>,
 "quantization_policy_sha256":<sha256 or null>,
 "retrieval_mode":<"semantic" | "weighted" | "none">,
 "run_letter":<"a" | "b" | "c" | "d" | "e" | "f" | "g">,
 "schema_version":3,"scope":<"whole" | "selection" | "graph">,
 "seed_lane":<"none" | "explicit" | derived policy ID>,
 "seed_policy_sha256":<sha256 or null>,"top_k":<positive integer>,
 "traversal_policy_sha256":<sha256 or null>,
 "vector_encoding":<"f32" | "i8" | "none">}
```

The line wrapping above is explanatory; the hash input is the compact
canonical encoding from section 2.2. `implementation_revision.source_sha256`
is the dirty-source hash from section 2.3 for a dirty build and null for a
clean build. `binary_sha256` is the hash of the exact executable bytes. Paths,
timestamps, host data, repetitions, and timing environment are excluded.
Collection/corpus IDs and version, `top_k`, and `evaluation_depth` MUST equal
the corresponding `collection.json` values. The implementation revision MUST
be identical across every A-G configuration in one artifact.

For retrieval runs, `metric` is the artifact's one frozen development-selected
VectorKit metric, `cosine` or `dot_product`, and MUST be identical across
A-C/E-G. `normalization` is exactly `unit_l2` for `cosine` and `none` for
`dot_product`, and `normalization_policy` is the corresponding exact section
3.8 object. For D, metric, normalization, and normalization policy are null.
`bm25_policy` is the exact section 3.8 object for C and G and null otherwise.
`candidate_limits` is
`{"keyword":null,"vector":null}` for A, B, D, E, and F. It contains the two
positive frozen development-selected limits for C and G, and the C and G
objects MUST be equal. `fusion_alpha` is the frozen finite f32 in `[0,1]` for C
and G and is null otherwise; C and G MUST use the same alpha.
`quantization_policy_sha256` is SHA-256 over the compact canonical encoding of
the exact section 3.8 quantization object for B, C, F, and G, and is null for
A, D, and E.

The remaining fields are fixed by run letter and lane:

| Run | `scope` | `retrieval_mode` | `vector_encoding` | `seed_lane` |
| --- | --- | --- | --- | --- |
| A | `whole` | `semantic` | `f32` | `none` |
| B | `whole` | `semantic` | `i8` | `none` |
| C | `whole` | `weighted` | `i8` | `none` |
| D | `selection` | `none` | `none` | `explicit` or one derived policy ID |
| E | `graph` | `semantic` | `f32` | `explicit` or one derived policy ID |
| F | `graph` | `semantic` | `i8` | `explicit` or one derived policy ID |
| G | `graph` | `weighted` | `i8` | `explicit` or one derived policy ID |

For A-C, all three graph/seed/traversal hashes are null. For D-G:

- `graph_schema_sha256` is SHA-256 over the exact bytes of
  `graph-schema.json`.
- `seed_policy_sha256` is SHA-256 over the exact bytes of
  `manifests/seed-policy.json`; the lane is selected separately by
  `seed_lane`.
- `traversal_policy_sha256` is SHA-256 over a canonical JSON array sorted by
  `query_id`. Each entry is exactly
  `{"query_id":<query ID>,"traversal":<the complete query traversal object>}`.
  The array covers the run's declared population from section 5.1, including
  derived-lane exclusions. D-G for the same seed lane therefore use the same
  traversal hash when their declared populations are equal; a retrieval-only
  subset may have a different hash.

The stable run ID is:

```text
v3-<letter>-<scope>-<mode>-<encoding>-<seed>-cfg-<hash12>
```

where:

- `letter` is lowercase `a` through `g`;
- `scope` is `whole`, `selection`, or `graph`;
- `mode` is `semantic`, `weighted`, or `none`;
- `encoding` is `f32`, `i8`, or `none`;
- `seed` is `na` when `seed_lane` is `none`, `explicit` when the lane is
  `explicit`, and otherwise the lowercase derived policy ID; and
- `hash12` is the first 12 lowercase hex characters of SHA-256 over the
  canonical run configuration object.

Run IDs MUST match `[a-z0-9][a-z0-9-]{0,95}`, be unique in the artifact, and
appear unchanged as the TREC tag and filename stem. A changed alpha, candidate
limit, BM25 policy, normalization policy, seed policy, traversal limit,
encoding, implementation revision, or collection version therefore creates a
different run ID.

Derived policy IDs used in run IDs MUST already be lowercase and match
`[a-z0-9][a-z0-9-]{0,31}`; they are never case-folded while constructing a run
ID. They are unique and exclude the four reserved values in section 3.8. No
implementation may substitute a manifest-object hash, parsed-object
hash, directory hash, or per-query subset for any exact byte/preimage hash
defined above.

### 4.3 TREC runs and chunk-to-document projection

Each TREC row is:

```text
query_id Q0 record_id rank trec_score run_id
```

The five separators are exactly one ASCII space and the literal second field is
`Q0`. Rank is a consecutive canonical positive base-10 integer starting at one for each
query. A query with no projected document, a pre-freeze exclusion, or an
invalid execution emits no row. Rows are ordered by
`query_id`, then increasing `rank`. Before projection,
chunk hits retain the exact native result order. Native ties MUST be resolved
by ascending stable `(record_id, chunk_key)`; ingestion MUST use the same
lexical identity order so internal-ID tie behavior cannot vary by input order.

The evaluator MUST exhaust the complete native chunk result allowed by the run
configuration before document projection; it MUST NOT request only `top_k`,
`evaluation_depth`, or an implementation-chosen overfetch. For A, B, E, and F,
the complete native result is every active chunk satisfying the filter and
scope, ordered by the native semantic score and stable tie-break. For C and G,
it is the complete fused ordering of the union of the first `vector` and
`keyword` component results selected by the two frozen candidate limits;
component lists themselves use native score order and the stable tie-break.
The union is duplicate-free by stable chunk identity. Exhaustion does not
remove the configured hybrid candidate limits; it means that every chunk in
the resulting bounded union is available to projection.

Document projection then scans that complete ordered chunk list and keeps only
the first chunk for each `record_id`, stopping after `evaluation_depth` unique
documents or actual exhaustion of the complete list. The retained chunk
identity and raw native score are written to `rust-results.json`. Duplicate
chunks from the same document never occupy multiple document ranks; the number
collapsed is reported per query. This procedure is the only
permitted chunk-to-document projection and overfetch policy.

TREC score is rank-derived:

```text
trec_score(rank) = evaluation_depth - rank + 1
```

It is a positive base-10 integer. Raw retrieval scores MUST NOT be put in the
TREC score column because equal or differently interpreted scores may be
reordered by external tools.

### 4.4 Graph selection and path files

A graph-selection file has exactly one row for each query whose execution
status in that D-G run is `valid`, no row for a pre-freeze exclusion or invalid
execution, and rows sorted by `query_id`. An invalid attempt is represented in
`rust-results.json` and `metrics.json`, never by a partly trustworthy selection
row. Every row contains exactly:

```text
{"active_corpus_chunks_before_filter":<integer>,"corpus_id":<ID>,
 "eligible_corpus_chunks_after_filter":<integer>,
 "generation_fingerprint":<sha256>,"matched_nodes":[<node identity>...],
 "projected_chunks_after_filter":<integer>,
 "projected_chunks_before_filter":<integer>,
 "projected_documents_after_filter":<integer>,"query_id":<ID>,
 "resolved_seed":<canonical seed>,"run_id":<run ID>,
 "seed_lane":<lane>,"seed_provenance":<object>,"seed_status":"resolved",
 "stale":false,"trace":{"diagnostics":<integer>,"result_count":<integer>,
 "seed_count":<integer>,"traversed_edges":<integer>,"visited_states":<integer>},
 "truncated_reason":<reason or null>}
```

Every count is a non-negative integer. `matched_nodes` is duplicate-free and
sorted by canonical bytes. `truncated_reason` is null or one of `max_hops`,
`max_visited`, `max_results`, or `max_working_bytes`.
`trace.result_count` equals the length of `matched_nodes`;
`projected_chunks_after_filter <= projected_chunks_before_filter`;
`projected_documents_after_filter <= projected_chunks_after_filter`; and
`eligible_corpus_chunks_after_filter <= active_corpus_chunks_before_filter`.

For an explicit lane, `seed_provenance` is exactly
`{"kind":"explicit","source_id":<section 3.8 value>,"transformation_id":<section 3.8 value>}`.
For a derived lane it is exactly
`{"alias_table_sha256":<sha256>,"kind":"derived","matched_aliases":[<alias match>...],"normalization_version":<ID>,"policy_id":<ID>,"policy_sha256":<sha256>,"policy_version":<ID>}`.
The policy hash is the section 4.5 derived-policy object hash.
An alias match is exactly:

```text
{"alias":<string>,"normalized_end":<integer>,"normalized_start":<integer>,
 "original_end":<integer>,"original_start":<integer>,"seed":<canonical seed>,
 "source":{"field":<field path>,"record_id":<record ID>}}
```

Offsets are non-negative, each end is at least its start, and the semantics are
section 6.2. Matches sort by `(normalized_start, normalized_end, original_start,
original_end, alias UTF-8 bytes, canonical seed bytes, record_id, canonical
field-path bytes)`; exact duplicate match objects are rejected.

`generation_fingerprint` is
SHA-256 over the compact canonical encoding of this exact preimage object:

```text
{"corpus_id":<corpus ID>,"corpus_state_sha256":<sha256>,
 "graph_state_sha256":<sha256>,"retrieval_state_sha256":<sha256 or null>,
 "schema_version":1}
```

`corpus_state_sha256` hashes a canonical JSON array of `{path,sha256}` objects
for `records.jsonl`, `manifests/preprocessing.json`, and
`manifests/chunking.json`, sorted by path. `graph_state_sha256` hashes the same
array form for `graph-schema.json` and
`manifests/graph-construction.json`. For E-G, `retrieval_state_sha256` hashes
this exact canonical object:

```text
{"bm25_policy_sha256":<sha256 or null>,
 "files":[{"path":"corpus-embeddings.f32.jsonl","sha256":<sha256>},
          {"path":"manifests/embedding.json","sha256":<sha256>}],
 "metric":<"cosine" | "dot_product">,
 "normalization":<"unit_l2" | "none">,
 "normalization_policy_sha256":<sha256>,
 "quantization_policy_sha256":<sha256 or null>,
 "vector_encoding":<"f32" | "i8">}
```

The `files` array is in lexical path order. Policy hashes use the compact
canonical section 3.8 objects. BM25 is null for E/F and non-null for G;
normalization is always non-null for E-G; quantization is null for E and
non-null for F/G. For D,
`retrieval_state_sha256` is null. The exact preimage object and resulting
fingerprint are also recorded in `manifest.json` as specified in section 4.6.
No delimiter concatenation or parsed-file re-encoding is permitted for these
hashes. Raw run-local generation IDs are validated in process but excluded
from deterministic artifacts.

`active_corpus_chunks_before_filter` is the raw active searchable corpus size.
`eligible_corpus_chunks_after_filter` is `N_q` from section 5.4.
`projected_chunks_before_filter` is the number of unique active stable chunk
identities projected from the graph before the query filter;
`projected_chunks_after_filter` is `C_q`, also unique by stable chunk identity;
and `projected_documents_after_filter` is the number of unique `record_id`
values among those chunks. Projection of the same chunk from multiple matched
nodes counts once. These counts MUST be retained even when a filter is null or
a scope is empty.

One graph-path JSONL row per distinct emitted path for each matched node from a
valid selection contains `query_id`, `run_id`, the matched node, depth,
zero-based `path_ordinal`, and the ordered canonical edge array from section
3.4. Rows sort by `(query_id, matched-node canonical bytes, path canonical
bytes)`. `path_ordinal` resets to zero for each
`(query_id, matched_node)` pair and is assigned only after that pair's paths
are sorted by canonical bytes. Exact duplicate paths for a pair are rejected.

The exact row fields are `query_id`, `run_id`, `matched_node`, `depth`,
`path_ordinal`, and `edges`; `depth` and `path_ordinal` are non-negative
integers, and `edges` is an array whose length equals `depth`. Zero-step matches
therefore have `depth: 0` and an empty `edges` array.

### 4.5 Diagnostic Rust results

`rust-results.json` contains exactly `collection_id`, `collection_version`,
`runs`, `schema_version`, and `seed_resolutions`; schema version is integer `3`.
Runs sort by `run_id`, and each is exactly
`{"queries":[<query result>...],"run_id":<run ID>,"status":<"valid" | "invalid_execution">}`.
Its query array has exactly one member for every member of that run's declared
population, sorted by query ID. The run status is `invalid_execution` if any
query has that status and `valid` otherwise. Run and query statuses MUST equal
the corresponding `metrics.json` statuses.

Each query result contains exactly:

```text
{"candidate_limits":{"keyword":<integer or null>,"vector":<integer or null>},
 "chunk_hits":[<chunk hit>...],"duplicate_collapse_count":<integer>,
 "execution_status":<"valid" | "excluded_pre_freeze" | "invalid_execution">,
 "filter":<section 3.6 AST or null>,"projected_documents":[<document hit>...],
 "query_id":<ID>,"selection_run_id":<run ID or null>,
 "status_reason":<reason or null>}
```

Candidate limits equal the run configuration. `selection_run_id` is null for
A-C and equals this run ID for D-G. `status_reason` is null for a valid row;
is `derived_seed_no_match` or `derived_seed_ambiguous` for a pre-freeze
exclusion; and is one of `stale_selection`, `generation_mismatch`,
`persistence_mismatch`, `non_deterministic_ranking`, `reload_mismatch`, or
`contract_violation` for an invalid execution. Excluded and invalid rows, and
every D row, have empty `chunk_hits` and `projected_documents` and zero
`duplicate_collapse_count`; partial results are never serialized as canonical
quality data.

Invalid attribution is exact. Any observed `stale_selection`,
`generation_mismatch`, `persistence_mismatch`, `reload_mismatch`, or
`non_deterministic_ranking` is a shared run/database failure: every member of
that run's execution population is rewritten to `invalid_execution`, even if
some queries completed before detection. Pre-freeze excluded rows remain
`excluded_pre_freeze`. A `contract_violation` is query-local only when the
violated predicate depends exclusively on that one query's frozen input and
result; otherwise, including inability to prove locality, it is run-wide and
rewrites every attempted row. Failure to start or finish one independently
scheduled query is a local `contract_violation`; aborting shared run setup or
state is run-wide.

Because one reason is serialized, simultaneous reasons use this precedence,
highest first: `generation_mismatch`, `stale_selection`,
`persistence_mismatch`, `reload_mismatch`, `non_deterministic_ranking`, then
`contract_violation`. A run-wide reason overrides a local reason. After
attribution, counts, query metrics, macro status counts, micro totals, Rust
results, TREC rows, selections, and paths are regenerated from the final
statuses; no earlier partial valid artifact survives.

A chunk hit contains exactly:

```text
{"bm25_normalized_score":<f32 or null>,"bm25_score":<f32 or null>,
 "chunk_key":<chunk key>,"fusion_score":<f32 or null>,
 "keyword_rank":<positive integer or null>,"matched_terms":[<string>...],
 "native_rank":<positive integer>,"record_id":<record ID>,
 "vector_normalized_score":<f32 or null>,
 "vector_rank":<positive integer or null>,"vector_score":<f32 or null>}
```

`chunk_hits` is the complete section 4.3 native chunk ordering before document
deduplication; `native_rank` is its one-based index. Stable chunk identities
are unique. Semantic runs require `vector_rank` and `vector_score`, set
`matched_terms` to `[]`, and set both BM25 fields, `keyword_rank`, and
`fusion_score` to null. Weighted runs require finite
`fusion_score`; component ranks and scores are present exactly when that chunk
occurred in that component's bounded list, and otherwise null. A normalized
component score is present exactly when its raw component score is present.

For a weighted hit with null `keyword_rank`, `matched_terms` is exactly `[]`.
For a non-null keyword rank, it is the duplicate-free lexical array of every
distinct query token produced by the run configuration's exact BM25 policy
that occurs at least once in that chunk's BM25-indexed token multiset. Values
are the normalized lowercase token strings, not original text spans. This is
the production `HybridTrace.matched_terms` value; no vector-only trace term,
unmatched query token, token frequency duplicate, or implementation-selected
diagnostic term may be added.

A projected document hit contains exactly
`{"chunk_key":<retained key>,"document_rank":<positive integer>,"native_chunk_rank":<positive integer>,"record_id":<record ID>,"score":<finite f32>}`.
It records the first retained chunk for each document in section 4.3 order;
document ranks are consecutive from one. `score` is vector score for semantic
runs and fusion score for weighted runs. `duplicate_collapse_count` is the
number of later chunk hits skipped because their record ID had already been
seen while scanning through the retained depth or exhaustion point.

`seed_resolutions` contains one row for every query in every non-empty `X_p`,
including failures, sorted by `(policy_id, query_id)`. Each row contains
exactly:

```text
{"alias_table_sha256":<sha256>,"candidate_seeds":[<canonical seed>...],
 "failure_reason":<"derived_seed_no_match" | "derived_seed_ambiguous" | null>,
 "matched_aliases":[<section 4.4 alias match>...],
 "normalization_version":<ID>,"policy_id":<ID>,
 "policy_sha256":<sha256>,"policy_version":<ID>,"query_id":<ID>,
 "selected_seed":<canonical seed or null>}
```

`policy_sha256` hashes the compact canonical encoding of that complete derived
policy object in `seed-policy.json`. Candidate seeds are distinct and sorted by
canonical bytes. Matches are the longest retained matches and use section 4.4
ordering. On success, `failure_reason` is null and `selected_seed` is the sole
candidate. On no match, both arrays are empty and selected seed is null. On
ambiguity there are at least two candidates, failure reason is
`derived_seed_ambiguous`, and selected seed is null. Explicit lanes do not
create resolver rows; their provenance is in graph selections.

The file contains no durations. Native scores are diagnostics and never used
to reconstruct TREC order.

### 4.6 Metrics, timing, exclusions, and manifest

`metrics.json` contains exactly `collection_id`, `collection_version`,
`exclusions`, `metric_definition_version`, `paired_comparisons`,
`publication_status`, `runs`, `schema_version`, and
`seed_resolution_coverage`. `schema_version` is integer `3` and
`metric_definition_version` is exactly `graph-retrieval-v3-r2`. Runs sort by
`run_id`; queries sort by `query_id`; paired comparisons sort by
`(scoped_run_id, baseline_run_id)`.

`publication_status` is `valid` only when every run is valid and is
`invalid_execution` otherwise. Every run contains exactly `counts`,
`declared_population_sha256`, `execution_population_sha256`, `macro`, `micro`,
`queries`, `run_id`, and `status`. `status` is `invalid_execution` if any query
has that status and `valid` otherwise.
The population hashes use section 2.3 and the formal populations in section
5.1. `counts` contains exactly the non-negative integers `attempted`,
`declared`, `excluded_pre_freeze`, `invalid_execution`, and `valid_execution`,
with these invariants:

```text
declared = excluded_pre_freeze + attempted
attempted = invalid_execution + valid_execution
```

Every frozen execution-population member counts as attempted; failure to start
or complete it is `invalid_execution`, never an omitted row.

`queries` has one row for every member of the declared population, including a
derived-lane pre-freeze exclusion. Each row contains exactly
`candidate_counts`, `execution_status`, `metrics`, and `query_id`.
`execution_status` is `valid`, `excluded_pre_freeze`, or `invalid_execution`.
`candidate_counts` is null for A-C, for a derived-lane exclusion, and for every
invalid execution; otherwise it is exactly
`{"eligible_chunks":N_q,"projected_chunks":C_q}` for D-G.

Every `metrics` object and every `macro` object contains exactly these keys:

```text
ap
candidate_complete_evidence
candidate_recall
candidate_reduction_ratio
complete_evidence_recall_at_10
complete_evidence_recall_at_5
empty_scope
judged_at_10
judged_at_5
mrr_at_10
ndcg_at_10
ndcg_at_5
path_accuracy
precision_at_5
recall_at_10
recall_at_5
success_at_1
supporting_document_recall_at_10
supporting_document_recall_at_5
truncated
truncated_max_hops
truncated_max_results
truncated_max_visited
truncated_max_working_bytes
```

Each per-query metric is exactly
`{"status":<section 5.1 status>,"value":<finite f64 or null>}`. A pre-freeze
excluded query uses `excluded_pre_freeze` and null for every metric. An invalid
execution uses `invalid_execution` and null for every metric. A valid execution
uses the applicability rules in sections 5.1-5.5. Each macro metric is exactly:

```text
{"denominator":<non-negative integer>,"numerator":<finite f64>,
 "status_counts":{"excluded_pre_freeze":<integer>,
                  "invalid_execution":<integer>,
                  "not_applicable":<integer>,"undefined":<integer>,
                  "valid":<integer>},
 "value":<finite f64 or null>}
```

Its status counts cover the complete declared population. `numerator` is the
sum of per-query values whose status is `valid`; `denominator` is the `valid`
count; `value` is their arithmetic mean or null when the denominator is zero.
The numerator is `0` when the denominator is zero. No other status contributes.

`micro` contains exactly these ten objects:

- `supporting_document_recall_at_5`,
  `supporting_document_recall_at_10`, and `candidate_recall`, each encoded as
  `{"matched_documents":<integer>,"required_documents":<integer>,"value":<f64 or null>}`;
- `candidate_reduction_ratio`, encoded as
  `{"candidate_chunks":<integer>,"eligible_chunks":<integer>,"value":<f64 or null>}`;
- `empty_scope_rate`, encoded as
  `{"empty_scopes":<integer>,"graph_valid_queries":<integer>,"value":<f64 or null>}`;
  and
- `truncation_rate`, `truncation_rate_max_hops`,
  `truncation_rate_max_results`, `truncation_rate_max_visited`, and
  `truncation_rate_max_working_bytes`, each encoded as
  `{"affected_queries":<integer>,"graph_valid_queries":<integer>,"value":<f64 or null>}`.

The list above contains ten objects: three evidence objects, one reduction
object, one empty-scope object, and five truncation objects. Evidence values
are `matched_documents / required_documents`; candidate reduction is
`eligible_chunks / candidate_chunks`; Empty-Scope Rate is
`empty_scopes / graph_valid_queries`; and each truncation rate is
`affected_queries / graph_valid_queries`. A value is null exactly when its
stated denominator is zero. Evidence totals use the alternative chosen
separately for that metric and result set under section 5.3. Candidate
reduction includes every valid graph execution, including `N_q` and zero
`C_q` for an empty scope. Empty-scope and truncation denominators are the valid
graph executions in that run. For a run to which a micro metric does not
apply, both totals are zero and value is null.

Each `paired_comparisons` entry contains exactly `baseline_run_id`, `metrics`,
`query_population_sha256`, `scoped_run_id`, `seed_lane`, and `status`. Its population is
the scoped run's frozen execution population from section 5.1, not a
post-execution intersection, and the hash MUST equal that scoped run's
`execution_population_sha256`. `metrics` contains the fourteen retrieval and
final-evidence keys from the list above: the ten ordinary retrieval metrics
from section 5.2 plus the four supporting/complete evidence metrics. Each is
exactly `{"baseline":<macro object>,"delta":<finite f64 or null>,"scoped":<macro object>}`.
The baseline macro is recalculated from A, B, or C per-query results on exactly
the frozen paired population; the scoped macro uses E, F, or G on that same
population. `delta` is `scoped.value - baseline.value` when both are finite and
null otherwise. The comparison status is `valid` only when both source runs are
valid. If either is invalid, its status is `invalid_execution`, every delta is
null, and the baseline/scoped macro objects remain required diagnostics over
the valid per-query rows. Diagnostic macros are never acceptance of an invalid
run.

Top-level `exclusions` contains exactly `by_lane`, `by_reason`, and `total`.
`by_reason` has one `{"count":<integer>,"reason":<allowed reason>}` entry for
every section 3.7 reason, including zero counts, sorted by reason. `by_lane`
has one `{"count":<integer>,"lane":"global"}` entry plus one entry for every
derived policy ID, including zero counts, sorted by lane. `total` equals both
the sum of `by_reason` counts and the sum of `by_lane` counts.

Top-level `seed_resolution_coverage` is an array sorted by `policy_id` with one
entry for every derived policy having non-empty `X_p`. Each entry is exactly
`{"declared":<|X_p|>,"failed":<|F_p|>,"policy_id":<p>,"successful":<|S_p|>,"value":<f64>}`.
`declared = failed + successful` and `value = successful / declared`. A policy
with empty `X_p` has no run and no coverage entry.

`timing-samples.jsonl` has two profiles:

- In `deterministic_quality`, it contains exactly one canonical row
  `{"profile":"deterministic_quality","status":"not_measured"}`. All other
  files are then eligible for byte comparison.
- In `performance`, each row contains environment ID, workload ID, run/stage,
  cold-or-warm state, repetition, sample index, and integer nanoseconds. Its
  exact object is
  `{"environment_id":<ID>,"nanoseconds":<non-negative integer>,"repetition":<non-negative integer>,"run_id":<run ID>,"sample_index":<non-negative integer>,"stage":<non-empty string>,"temperature":<"cold" | "warm">,"workload_id":<ID>}`. Rows
  sort by `(environment_id, workload_id, run_id, stage, temperature,
  repetition, sample_index)`. Performance samples are observations and are not
  expected to be byte-identical.

`manifest.json` contains exactly `collection_id`, `collection_version`,
`determinism_context`, `determinism_environment`, `deterministic_files`,
`files`, `generation_fingerprints`, `implementation_revision`,
`metric_definition_version`, `population_hashes`, `profile`,
`publication_status`, `run_configurations`, and `schema_version`.
`schema_version` is integer `3`; `profile` is `deterministic_quality` or
`performance`; metric version and publication status equal `metrics.json`;
and `implementation_revision` equals the exact object in every run
configuration. There is no timestamp.

`run_configurations` sorts by `run_id` and has one entry per canonical run,
each exactly:

```text
{"configuration":<section 4.2 exact object>,
 "declared_population_sha256":<sha256>,
 "execution_population_sha256":<sha256>,
 "generation_fingerprint":<sha256 or null>,
 "logical_run_sha256":<sha256>,"run_id":<run ID>}
```

The run ID is section 4.2's hash-derived ID. Population hashes equal
`metrics.json`. Generation fingerprint is null for A-C and the one preimage
fingerprint bound to that D-G combined database otherwise. All valid selection
rows for the run use it. `logical_run_sha256` hashes the compact canonical run
configuration after removing only its `implementation_revision` member; it is
the exact cross-context matching key in section 4.7.

`population_hashes` is the sorted-by-run-ID array of exactly
`{"declared":<sha256>,"execution":<sha256>,"run_id":<run ID>}` and duplicates
the run entries intentionally for a small independent population index.

`generation_fingerprints` is an array sorted by `fingerprint`; each entry is
exactly `{"fingerprint":<sha256>,"preimage":<section 4.4 exact object>}` and
there is one unique entry for every non-null fingerprint in
`run_configurations`, including an invalid run that emits no valid selection
row. `determinism_environment` is exactly:

```text
{"cpu_architecture":<string>,"cpu_features":[<lexically sorted strings>],
 "execution_threads":<positive integer>,
 "floating_point_mode":"round_to_nearest_ties_to_even","locale":"C",
 "os_build":<string>,"runtime_flags":[<lexically sorted strings>]}
```

Quality execution MUST set and verify the stated floating-point mode and
locale. Every free string is non-empty, and both arrays are duplicate-free.
`determinism_context` is exactly:

```text
{"binary_sha256":<sha256>,"environment_sha256":<sha256>,
 "runtime_id":<string>,"runtime_version":<string>,"target_triple":<string>}
```

`environment_sha256` hashes the compact canonical encoding of
`determinism_environment`. `binary_sha256` equals every run configuration's
implementation binary hash. Runtime ID/version name the language/runtime and
standard-library implementation used by that binary; `target_triple` is the
exact compilation target triple.

The manifest's `files` array contains an object with exactly `path`, `bytes`,
and `sha256` for every regular artifact file except `manifest.json` itself and
is sorted by `path`; `bytes` is a non-negative integer. This
explicit exclusion prevents a self-referential digest. The
`deterministic_files` array is a lexical list of paths drawn from `files`; in
`deterministic_quality` it equals the complete `files` path set. In
`performance` it equals that set minus `timing-samples.jsonl`.

`exclusions.jsonl` never acquires execution-time rows in a canonical quality
run. Runtime failures are `invalid_execution` statuses and may invalidate the
run; they are not relabeled as exclusions. Test-result-driven exclusions are
forbidden.

### 4.7 Byte-identical reruns and reload equivalence

Two `deterministic_quality` executions with identical collection bytes, run
configuration objects, and `determinism_context` MUST produce byte-identical
bytes for every file in section 4.1. Equality of `determinism_context` means
the same executable bytes, compilation target, runtime/standard-library
identity and version, CPU architecture/features, floating-point mode, locale,
OS build, execution-thread count, and runtime flags. Directory entry metadata
is irrelevant. The second run MUST be
created in a fresh output directory; stale files are an error, not silently
retained.

Byte identity is not required across different determinism contexts. The exact
cross-context comparator constructs this in-memory portability view; it is not
another artifact:

1. In each manifest, build the bijection from every `run_id` to its
   `logical_run_sha256`. Equal logical hashes pair runs. Missing, duplicate, or
   unequal logical-run sets fail comparison.
2. Rename `runs/<run-id>.trec`, `graph-selections/<run-id>.jsonl`, and
   `graph-paths/<run-id>.jsonl` in memory by replacing the filename stem with
   the mapped logical hash. Replace TREC run tags through the same map.
3. In parsed JSON, replace the value of every exact key `run_id`,
   `selection_run_id`, `baseline_run_id`, and `scoped_run_id` with its mapped
   logical hash. Null remains null. An unmapped non-null value fails.
4. For `manifest.json`, remove top-level `determinism_context`,
   `determinism_environment`, `implementation_revision`, `files`, and
   `deterministic_files`. Inside each `run_configurations` member, remove only
   `configuration.implementation_revision`; retain the remapped run ID,
   logical hash, populations, generation fingerprint, and every other
   configuration field. Apply the run-ID replacement to `population_hashes`.
5. Replace the removed manifest file arrays with one comparison-only lexical
   `logical_paths` array made from the actual section 4.1 file set after step
   2. `manifest.json` itself is included. Raw context-bound byte counts and file
   digests are not projected or compared.
6. Compare canonical copied judgments/exclusions, string values, stable
   identities, ranks, selections, paths, statuses, reasons, integer totals,
   population hashes, context-independent hash preimages and their hashes, and
   `logical_paths` exactly. Compare finite native `f32` fields within absolute
   tolerance `1e-6` and finite calculated `f64` metric fields within absolute
   tolerance `1e-12`; null and metric status must still match exactly.

No other key, path component, hash, or configuration member is removed or
rewritten. This procedure accounts for nested paired IDs, population indexes,
run-keyed filenames, manifest file hashes, and binary identity without an
implementation-selected projection. A cross-context comparison is a
portability check, not the section 12 byte-rerun gate.

Every canonical run is executed before save and after validated reload.
Rankings, selections expressed in stable identities, paths, truncation reason,
projection counts, generation fingerprints, and metrics MUST be identical.
Raw generation IDs are checked in process for correct binding and are not
serialized in this profile. A stale or generation-mismatched selection marks
only the receiving D, E, F, or G run, and every attempted query in that run, as
`invalid_execution` under section 4.5; it does not retroactively change another
run's statuses. Persistence, reload, and non-deterministic-ranking failures
similarly invalidate every attempted query only in each run using the affected
database. Any such failure fails the overall artifact publication/Phase gate,
even when isolated to one run. Required macro and micro objects remain present,
with no valid attempted row in that run, and its paired comparisons are status
`invalid_execution`; it MUST NOT be published or compared as valid.

## 5. Formal metric semantics

### 5.1 Populations and status

Let `Q` be exactly the set of query IDs in `queries.jsonl`. Global exclusions
are outside `Q`. Define these frozen sets from collection bytes, before any run
executes:

```text
R      = {q in Q | q.tasks contains retrieval}
X_exp  = {q in Q | q.explicit_seed is non-null}
X_p    = {q in Q | q.derived_seed_policy_id = p}
F_p    = {q in X_p | exclusions has (q,p,derived_seed_no_match or
                                     derived_seed_ambiguous)}
S_exp  = X_exp
S_p    = X_p minus F_p
```

`R` MUST be non-empty; a collection with no retrieval query is invalid. Thus
A, B, and C always exist.

The declared population is the set represented in a run's `metrics.json`
entry. The execution population is the frozen subset that the engine attempts:

| Run/lane | Declared population | Execution population |
| --- | --- | --- |
| A, B, C | `R` | `R` |
| D explicit | `X_exp` | `S_exp` |
| D derived policy `p` | `X_p` | `S_p` |
| E, F, G explicit | `X_exp intersect R` | `S_exp intersect R` |
| E, F, G derived policy `p` | `X_p intersect R` | `S_p intersect R` |

There is one D run for `explicit` exactly when `X_exp` is non-empty and one D
run for derived policy `p` exactly when `X_p` is non-empty. There is one E, F,
and G run for a lane exactly when that lane's declared population intersected
with `R` is non-empty. No run exists for an empty declared population; an
existing run may still have an empty execution population when every declared
derived query is pre-freeze excluded. Different derived policy IDs are
different lanes and run IDs. `retrieval-valid` means membership in `R` after
collection validation. `evidence-valid` for a run means valid execution plus
an `evidence` task and its required judgment row. `path-valid` for a run means
valid graph execution plus a `path` task and an expected-path row for that
run's seed lane. `graph-valid` means membership in a D-G execution population
and a `valid` execution status. In section 6.2, `otherwise graph-eligible for
policy p` means exactly `X_p`; derived resolution coverage is `|S_p| / |X_p|`.
These are definitions, not implementation-selected filters.

An all-excluded derived run is status `valid`: `attempted=0`, every declared
row is `excluded_pre_freeze`, macro denominators are zero, applicable micro
denominators are zero with null values, and every required TREC/selection/path
file is zero bytes. Exclusion alone is not an execution failure.

Each per-query metric has `value` or null plus exactly one status:

- `valid`: the metric applies, has a finite value, and enters its macro
  denominator;
- `undefined`: the metric applies to a valid execution but its formula has a
  zero denominator; its value is null and it does not enter the macro
  denominator;
- `not_applicable`: the run does not produce the required result type, the
  query did not declare the metric's task, or an optional expected-path
  judgment is absent; its value is null;
- `excluded_pre_freeze`: the query is in a derived declared population but not
  its execution population; its value is null; or
- `invalid_execution`: the run attempted the query but violated the contract;
  its value is null, the affected run fails, and the value is never omitted or
  converted to zero.

For a `valid` execution, ordinary retrieval metrics apply only in A-C and E-G
and require the `retrieval` task. Supporting/complete evidence metrics apply
only in retrieval runs and require `evidence`. Candidate evidence metrics
apply only in D-G and require `evidence`. Candidate Reduction Ratio, Empty
Scope, and all truncation indicators apply to every valid D-G execution. Path
Accuracy applies only to a path-valid D-G execution. All other combinations
are `not_applicable`. Candidate Reduction Ratio for `C_q=0` is the required
`undefined` per-query case in V3; Empty Scope is still `valid` with value `1`.

A query intended for `R` but having no positive qrel MUST instead be globally
excluded before `queries.jsonl` is frozen with `no_relevant_documents`.
Retaining it in `R` makes the collection invalid. Missing evidence for a query
declaring `evidence` also makes the collection invalid; it is not repaired at
execution time. Missing expected paths makes only Path Accuracy
`not_applicable` in the lane lacking a row.

An empty graph selection and a truncated selection are valid, scored outcomes.
A stale selection or a selection from a different corpus/generation is
`invalid_execution` for the receiving run under section 4.7. If a positive
qrel or evidence document does not satisfy the query's declared metadata
filter in the canonical corpus, the adapter MUST globally exclude the query
before freeze with `filter_label_conflict`. If the document satisfies the
filter but the engine omits it, the omission remains a scored retrieval
failure.

The normative aggregate for every quality rate is the arithmetic macro mean
over `valid` per-query values. `undefined`, `not_applicable`,
`excluded_pre_freeze`, and `invalid_execution` never enter its numerator or
denominator. Required micro values are exactly those in section 4.6 and never
replace macro quality metrics.

All calculated metric arithmetic is IEEE-754 binary64, round-to-nearest
ties-to-even after every named operation. Integer counts are accumulated as
integers and converted once to binary64 immediately before division. Gain
`2^relevance - 1` is formed as an exact integer and converted once. `log2` MUST
be the correctly rounded binary64 result. Multiplication, division, addition,
subtraction, square root, and logarithm are separate operations: fused
multiply-add, extended-precision intermediates, reassociation, pairwise or
compensated summation, and implementation-default parallel reduction are
forbidden.

Within a per-query metric, sums proceed left-to-right in increasing document
rank. IDCG uses its already defined grade/record order. Macro numerators add
valid per-query values left-to-right in lexical `query_id` order, followed by
one division by the integer denominator. Micro count totals add integer counts
in the same query order and perform one final division. Paired baseline and
scoped macros use the same order over their frozen population; delta is one
binary64 subtraction `scoped - baseline`. Alternative evidence ratios are
compared as exact integer fractions by cross multiplication before the section
5.3 tie-break, then only the selected fraction is converted and divided.
These rules also govern every displayed worked-example intermediate. Section
2.2 serializes the resulting binary value.

### 5.2 Existing retrieval metrics

For query `q`, let `rel_q(d)` be its integer qrel grade, `R_q^K` the first `K`
unique projected documents, and `L_q = {d | rel_q(d) >= 1}`.

At one-based rank `i`:

```text
gain(i) = 2^rel_q(R_q[i]) - 1
discount(i) = log2(i + 1)
DCG@K = sum(i=1..K, gain(i) / discount(i))
NDCG@K = DCG@K / IDCG@K
```

`IDCG@K` sorts all judged grades descending, with record ID as the deterministic
tie-break. V3 retrieval-valid queries have positive IDCG. Report NDCG@5 and
NDCG@10.

```text
Recall@K    = |R_q^K intersect L_q| / |L_q|
Success@1   = 1 if R_q^1 intersects L_q, else 0
Precision@5 = |R_q^5 intersect L_q| / 5
MRR@10      = 1 / min{i <= 10 | R_q[i] in L_q}, or 0 if none
```

Precision uses the fixed cutoff denominator even when fewer than five results
are returned. Report Recall@5, Recall@10, Success@1, Precision@5, and MRR@10.

Average Precision uses the complete emitted document run up to
`evaluation_depth`:

```text
AP(q) = (1 / |L_q|) * sum(i=1..evaluation_depth,
          indicator[R_q[i] in L_q] * Precision@i)
MAP   = macro mean of AP(q)
```

Unretrieved relevant documents contribute zero. `AP` is per query; `MAP` is
the macro aggregate.

Judgment coverage counts grade-zero rows as judged:

```text
Judged@K = judged returned documents in first K / min(K, returned count)
```

It is `0` when no document is returned. Report Judged@5 and Judged@10. These
definitions intentionally match the existing V2 Rust artifact evaluator and
its TREC-compatible projection, including `2^rel - 1` NDCG gain and fixed-cutoff
Precision@5.

### 5.3 Evidence and candidate metrics

For an evidence-valid query, let `E_q = {E_q1, ..., E_qm}` be its alternative
complete evidence sets. For any document set `S`, define:

```text
evidence_recall(q, S) = max(E in E_q, |S intersect E| / |E|)
evidence_complete(q, S) = 1 if any E in E_q is a subset of S, else 0
```

Supporting Document Recall@K is `evidence_recall(q, R_q^K)`. Complete Evidence
Recall@K is `evidence_complete(q, R_q^K)`. Report both at K=5 and K=10; Complete
Evidence Recall@10 is the primary graph-quality metric.

Candidate Recall is `evidence_recall(q, C_q^doc)`, where `C_q^doc` is the
unique document set owning graph-projected active chunks after intersection
with the unchanged metadata filter and before ranking. Candidate Complete
Evidence is also reported diagnostically using `evidence_complete`.

When alternatives tie, choose the one with the highest numerator, then the
smallest cardinality, then lexically smallest canonical array for micro totals.
Normative macro scores are the per-query maxima above. Micro evidence recall is
the sum of matched documents divided by the sum of required documents for the
chosen alternatives; it is diagnostic. Choose an alternative independently
for Supporting Document Recall@5, Supporting Document Recall@10, and Candidate
Recall because their result sets differ. Complete-evidence metrics have no
separate micro object.

### 5.4 Candidate reduction, empty scope, paths, and scoped NDCG

Let `N_q` be the number of active searchable corpus chunks satisfying the
metadata filter before graph scoping, and `C_q` the number of unique projected
active chunks after graph scoping and that same filter.

```text
Candidate Reduction Ratio(q) = N_q / C_q, when C_q > 0
Empty Scope(q) = 1 if C_q = 0, else 0
```

For an empty scope, the ratio has status `undefined` and value `null`, never
infinity. Report per-query `N_q`, `C_q`, and the ratio; macro reduction is the
arithmetic mean over `valid` non-empty ratios. The normative cross-query
reduction is the micro ratio `sum N_q / sum C_q` over every graph-valid query,
including empty scopes, when the summed candidate count is nonzero. Its value
is null when that aggregate denominator is zero. Empty-Scope Rate is
`sum Empty Scope(q) / number of graph-valid queries` and includes
resolver-success queries whose traversal projects no searchable chunk.

Path Accuracy applies only where expected paths exist. An actual path matches
an expected path only when path length and every ordered edge's source node,
target node, relationship type, direction, and occurrence ordinal are exactly
equal. The per-query score is `1` if at least one emitted path matches at least
one allowed expected path, otherwise `0`. Multiple expected paths are valid
alternatives; extra emitted paths do not create a match. Aggregate Path
Accuracy is the macro mean over path-valid queries. Exact graph-selection and
path-set equality remain separate correctness checks and catch unexpected
extra output.

Scoped NDCG@10 is the section 5.2 NDCG@10 computed on E, F, or G. It is not a
new gain function. It MUST be reported by seed lane and paired against the
corresponding A, B, or C run on the identical frozen scoped execution
population. An `invalid_execution` fails that paired comparison and the
publication gate; it does not shrink the population.

### 5.5 Truncation

For reason `r` in `max_hops`, `max_visited`, `max_results`, and
`max_working_bytes`:

```text
Truncation Rate(r) = queries whose graph result reports r /
                     graph-valid executed queries
```

The current engine reports at most one reason per query. If a future engine
reports multiple reasons, each reason receives its own indicator and the
overall rate counts the query once. Report the overall rate, all four
reason-specific rates, and raw counts. A truncated query remains in candidate,
evidence, path, empty-scope, and retrieval metric denominators using the actual
partial result.

For each graph-valid query, `truncated` is `1` when any reason is present and
`0` otherwise. Each `truncated_<reason>` metric is `1` exactly when that reason
is present and `0` otherwise. Their macros and the identically named micro rate
objects therefore use the same graph-valid denominator.

### 5.6 Required output levels

`metrics.json` MUST distinguish:

- per-query values and statuses;
- normative macro means over valid queries;
- explicitly named micro totals/ratios;
- full-population whole-corpus metrics;
- paired whole/scoped metrics on the exact seed-lane population; and
- invalid and excluded counts without folding them into zero-valued means.

No aggregate may combine explicit and derived seed lanes.

The exact per-query, macro, micro, exclusion, and paired-comparison objects are
normative in section 4.6. Implementations MUST emit every required key,
including zero-denominator null objects, and MUST NOT add convenience aliases,
omit zero-count statuses, or serialize a different micro matrix.

### 5.7 Normative worked example

For one valid query, suppose the qrels are `d1:2`, `d2:1`, and `d3:0`; the
projected ranking is `[d2,d3,d1]`; the alternative evidence sets are
`[[d1,d2],[d1,d4]]`; the graph candidate documents are `{d1,d4}`; and
`N_q=10`, `C_q=2`. Then:

```text
NDCG@5 = (1 + 3/log2(4)) / (3 + 1/log2(3))
       = 0.6885288809404666
Recall@5 = 2/2 = 1
Success@1 = 1
Precision@5 = 2/5 = 0.4
MRR@10 = 1
AP = (1 + 2/3) / 2 = 0.8333333333333333
Judged@5 = 3/min(5,3) = 1
Supporting Document Recall@5 = 1
Complete Evidence Recall@5 = 1
Candidate Recall = max(1/2,2/2) = 1
Candidate Reduction Ratio = 10/2 = 5
Empty Scope = 0
```

The second evidence alternative wins Candidate Recall; documents from
different alternatives are not pooled.

For the normative empty-scope variant, keep the same judgments, evidence,
filter, and `N_q=10`, but graph projection produces `C_q=0`. Runs E-G consume
that actual empty scope, so their graph-scoped projected ranking is the empty
list; the earlier `[d2,d3,d1]` ranking is replaced, not retained. The complete
variant is:

```text
NDCG@5 = 0
Recall@5 = 0/2 = 0
Success@1 = 0
Precision@5 = 0/5 = 0
MRR@10 = 0
AP = 0
Judged@5 = 0
Supporting Document Recall@5 = 0
Complete Evidence Recall@5 = 0
Candidate Recall = max(0/2,0/2) = 0
Candidate Complete Evidence = 0
Candidate Reduction Ratio status = undefined
Candidate Reduction Ratio value = null
Empty Scope = 1
```

All zero-valued metrics above have status `valid`; only Candidate Reduction
Ratio has status `undefined`. No whole-corpus fallback occurs. If this query
had no positive qrel, it would instead be a global pre-freeze exclusion and
would not appear in `queries.jsonl` or any run macro denominator.

## 6. Graph seed contract

### 6.1 Explicit structured seeds

An explicit seed represents application-provided structure. It MUST be present
in `queries.jsonl`, conform to the seed union in section 3.6, and be created
from upstream query inputs or a documented application scenario without using
qrels, evidence, expected paths, or any retrieval result. An adapter MUST
record the upstream field and transformation that produced it.

An invalid or nonexistent explicit node is a collection construction error,
not a cue to substitute a gold document. Explicit-lane run IDs use
`seed=explicit` and are reported independently.

### 6.2 Deterministic exact-alias-derived seeds

Each derived policy freezes an alias table built only from graph-queryable
canonical record fields named in `seed-policy.json`. Each alias row maps source
field provenance to one structured seed. Qrels, supporting-fact labels,
expected paths, test results, LLMs, learned entity extractors, fuzzy matching,
stemming, and embedding similarity are prohibited.

Normalization is frozen as follows:

1. Validate UTF-8 and normalize to Unicode NFC.
2. Apply Unicode 15.1 default full case folding.
3. Map every Unicode White_Space code point to ASCII space.
4. Collapse consecutive spaces and trim leading/trailing space.
5. Preserve punctuation, diacritics, and all other code points.

The manifest MUST pin the Unicode tables or library artifact checksum so a
runtime Unicode upgrade cannot change output.

Normalize aliases and query text identically. An alias matches a contiguous
normalized query substring only when both ends are the string boundary or a
boundary between a Unicode letter/number and a non-letter/non-number. The
resolver then:

1. collects all matches with original and normalized offsets;
2. measures each matched alias by the number of Unicode scalar values in its
   fully normalized alias after all five normalization steps and retains only
   matches with that greatest normalized scalar length;
3. canonicalizes the distinct structured seeds they produce; and
4. succeeds only if exactly one distinct seed remains.

Zero matches deterministically fail as `derived_seed_no_match`; more than one
distinct seed fails as `derived_seed_ambiguous`. Multiple longest matches for
the same seed succeed once. No shorter-alias fallback is allowed after an
ambiguous longest match.

For every query in `X_p`, the resolver records policy ID/version/hash,
alias-table hash, normalization version, all retained matched aliases and
offsets, candidate seeds, selected seed or failure reason, and source
record/field provenance.
Offsets are half-open Unicode-scalar indexes. Normalized offsets address the
fully normalized query. Original offsets address the smallest half-open span
of original query scalars that contributed to the normalized match; the
normalizer MUST retain this contribution map through case-fold expansion and
whitespace collapse. UTF-8 byte offsets MAY also be emitted diagnostically but
cannot replace the scalar offsets.

V3 chooses **exclusion, not retrieval fallback**, for derived failures. The
derived-lane exclusion list and its hash MUST be frozen before configuration
tuning. For each policy `p`, the failure-list hash is the section 2.3
query-population hash over the ordered IDs in `F_p` and is stored in
`manifests/seed-policy.json` together with the `X_p` and `S_p` population
hashes. Failed queries still count in published derived-seed resolution
coverage. For policy `p`, its denominator is exactly `X_p`, its numerator is
exactly `S_p`, and the value is `|S_p| / |X_p|` under section 5.1. The value is
null only when `X_p` is empty, in which case no run for `p` exists. Failed
queries remain in whole-corpus runs when they are members of `R`; they are not
included in conditional derived graph metrics.
There is no silent fallback to a gold title, explicit seed, LLM, or
whole-corpus result inside runs D through G.

## 7. Leakage prevention

### 7.1 Information classes

Construction inputs are upstream corpus content, public document structure,
hyperlinks or structured relationships, stable upstream IDs, and upstream
query text/declared application constraints. Judgment inputs are qrels,
supporting documents/facts, and expected paths. Evaluation outputs are rankings,
scores, selections, traces, metrics, failures, and timings.

Judgment inputs and evaluation outputs MUST NOT influence:

- corpus or chunk selection;
- graph node or edge construction;
- alias-table construction or seed generation;
- traversal steps, relationship direction, or per-query hop limits;
- metadata filters;
- graph or retrieval candidate limits;
- weighted-fusion alpha;
- embedding model, revision, preprocessing, or vector encoding; or
- test-query inclusion after configuration tuning begins.

Every construction and policy manifest records only allowed input hashes. The
builder MUST fail if a judgment or results path is supplied to a construction
stage.

### 7.2 Permitted split use and lock sequence

The required sequence is:

1. Pin upstream release, license, archive URL, and checksum.
2. Select and freeze the global under-50K document universe without judgments.
3. Build canonical records, chunks, graph schema/edges, and alias table without
   judgments.
4. Use upstream labels once to determine which questions have complete gold
   evidence in that already-frozen corpus and to emit qrels/evidence/path
   judgment files.
5. Resolve deterministic seed failures and all schema/filter inconsistencies.
6. Freeze development and test query-ID lists, exclusions, collection bytes,
   and hashes **before** any configuration tuning.
7. Use only development queries, labels, and results to choose one global or
   predeclared category-level configuration for alpha, candidate limits,
   traversal limits, and embedding choice. V3 has no derived-seed fallback
   policy to tune.
8. Freeze the run-configuration hashes.
9. Execute the locked test split once for final reporting. Test labels are
   loaded by the evaluator only after rankings and graph results have been
   finalized.

Development results MAY change configuration and require a new configuration
hash. They MUST NOT change the frozen corpus or test query list. Test results
MUST NOT trigger query exclusion, per-query tuning, graph edits, seed edits, or
rerunning with a preferred configuration. A defect fix requires a new
implementation revision and a disclosed complete rerun; a collection defect
requires a new collection version.

## 8. Canonical execution matrix

The canonical runs are:

| Run | Scope and ranking | Encoding | Seed |
| --- | --- | --- | --- |
| A | Whole-corpus semantic exact | F32 | none |
| B | Whole-corpus semantic exact | I8 scalar-quantized | none |
| C | Whole-corpus weighted hybrid | I8 scalar-quantized | none |
| D | Graph selection only | none | explicit and derived, separate |
| E | Graph-scoped semantic exact | F32 | explicit and derived, separate |
| F | Graph-scoped semantic exact | I8 scalar-quantized | explicit and derived, separate |
| G | Graph-scoped weighted hybrid | I8 scalar-quantized | explicit and derived, separate |

C and G use the current compact product encoding, I8, so the flagship C-versus-G
comparison changes only structural scope. RRF remains diagnostic and cannot
replace C or G.

The following inputs MUST be byte-identical or canonically equal across A-G:

- collection/corpus version, canonical records/chunks, query text, source F32
  corpus/query embeddings, qrels, evidence, metadata filters, evaluation depth,
  top K, active/deleted corpus state, preprocessing, chunking, embedding model,
  and implementation revision;
- graph schema and construction for D-G; and
- metric-specific runtime normalization across A-C/E-G, plus weighted alpha,
  BM25 policy, and vector/BM25 candidate limits between C and G.

Intentional differences are only whole versus graph scope, F32 versus I8
storage/scoring, semantic versus weighted hybrid ranking, and explicit versus
derived seed source. I8 is produced from the same F32 inputs with the same
metric-specific section 3.8 normalization first and the same quantization
policy second in B, C, F, and G.

A-C use declared and execution population `R`. D runs once for every present
explicit or derived seed lane; E-G run once for every such lane that has at
least one declared retrieval query. Their exact declared and execution
populations are the section 5.1 table, including derived exclusions in the
declared population but never in execution. No implementation may broaden a
lane to unseeded queries, drop retrieval-only seeded queries, or infer
membership from available judgments.

For fair ablations, each paired comparison uses exactly the frozen E, F, or G
execution population and recalculates A, B, or C from already finalized
per-query results on that identical ID set. The pairs are A-E, B-F, and C-G for
the same seed lane. Runtime success/failure MUST NOT change that population or
trigger a new intersection. No result from D may alter E-G configuration.

Graph projection precedes retrieval metadata-filter intersection, and both
counts are retained. E-G MUST consume a generation-bound selection produced by
their own combined database. Their stable selection identities and paths MUST
equal D's graph-only result, proving capability composition without pretending
that the engines or generations are identical.

## 9. Device and performance protocol

### 9.1 Targets supported by repository evidence

The headline device is **iPhone 17 Pro Max (`iPhone18,2`)**. Repository reports
identify it, iOS 26.5.1, release Swift/optimized Rust builds, repeated 24K and
50K measurements, and memory/persistence behavior. A new public run MUST still
record the actual OS and toolchain; the old OS value is evidence for selection,
not a requirement to downgrade. The selection evidence is
`docs/product/reports/iphone-17-pro-max-benchmark-report.md` and
`docs/product/reports/iphone-17-pro-max-memory-budget-report.md`.

The pinned development Mac is the documented **Apple M1 Max**. Mac results are
development regressions, not substitutes for physical iPhone results. The
graph-specific evidence is
`docs/product/reports/graph-m3-benchmark-report.md`.

The repository owner selected **iPhone 14 Pro Max with iOS 26 or later** as the
conservative physical device. It is separate from the headline-device evidence
and MUST:

- be a physical iPhone supported by the app's then-current minimum iOS and
  VectorKit binary;
- be at least two hardware generations older than the headline device;
- have less memory and/or weaker vector capabilities than the headline device;
- disclose whether AArch64 dot-product and the selected SIMD backend are
  available; and
- complete the 10K correctness workload without thermal or memory failure.

The benchmark harness MUST record the actual hardware identifier, RAM class,
AArch64 dot-product result, and selected SIMD backend from the tested device at
runtime. Do not infer those capabilities from the marketing model name. Until
the first qualifying run records them, they are pending performance evidence,
not an unresolved Phase 0 product decision.

### 9.2 Workloads

Define immutable `10k-384d-v3`, `25k-384d-v3`, and `50k-384d-v3` workloads
containing exactly 10,000, 25,000, and 50,000 active chunks. Record and graph
counts may differ but are fixed in each manifest. Each includes explicit
references and reference collections; one-, two-, and three-hop traversals;
cycles, repeated references, missing optional references, deleted records;
graph-plus-text queries; metadata-filter intersections; semantic and exact-name
queries; and semantically similar distractors.

All three use the same deterministic generator/policy, 384-dimensional F32
source embeddings, top K 10, and both F32 and I8 retrieval configurations.
Additional 768d rows are optional and separately named. Workloads execute
offline with precomputed embeddings. A model-inclusive benchmark is a distinct
end-to-end profile and MUST report embedding separately.

### 9.3 Builds, sampling, and percentiles

- Use release Swift code and optimized Rust/XCFramework code with assertions
  and tracing configured as in the shipping package.
- Record device model/identifier, OS build, toolchain, VectorKit revision,
  workload hash, power state, battery range, low-power mode, thermal state,
  free storage, and foreground/background conditions.
- Run one workload/configuration per fresh app process. Disable network use and
  competing foreground work. Abort and repeat a run if the OS reports a serious
  or critical thermal state.
- Warm query stages with 100 complete fixed-sequence queries, excluded from
  measurement, then record 1,000 samples per configuration.
- For build, save, load, and read-only validation, use three discarded warmups
  followed by 20 measured samples on fresh uniquely named directories.
- Cold open/load uses 20 fresh-process samples with no warmup. Warm query
  distributions begin only after load, validation, and warmup complete.
- Peak memory uses one scenario per fresh process, samples process RSS at 1 ms,
  and repeats five times. Report process baseline, peak RSS, and peak delta.
- Run at least three final sessions per device/configuration. Report each
  session and the median of session P95s for gates; do not pool away session
  variance.

For sorted `n` samples and percentile `p`, use nearest-rank:

```text
index = max(1, ceil(p * n)) - 1
percentile = sorted_samples[index]
```

Report P50, P95, P99, minimum, maximum, arithmetic mean, and sample count.
Integer nanoseconds are the raw unit. A total-operation percentile is measured
around the total operation and MUST NOT be calculated by summing component
percentiles.

### 9.4 Required stage and resource separation

Warm query timing records, separately:

1. seed resolution;
2. graph traversal;
3. graph-to-chunk projection;
4. metadata-filter intersection;
5. semantic or hybrid ranking;
6. result hydration; and
7. total graph-to-hydrated-result latency.

Also report build, save, cold load, warm/repeated load where meaningful,
read-only validation, and reload-equivalence checking. Embedding is absent from
retrieval-only total.

Persisted bytes are reported for corpus, graph, vectors/quantization metadata,
lexical/BM25, manifest/validation metadata, and complete directory. Memory
reports process baseline, peak/delta for build, save, load, validation, query,
and compaction or maintenance when measured. Never infer per-component resident
memory by subtracting unrelated sequential runs.

### 9.5 Graph-free regression gate

Installing graph support MUST NOT route graph-free queries through graph-aware
dispatch or materially regress the graph-free hot path. Use the existing gate:

- pinned 10K x 384d deterministic fixture, top K 10;
- prebuilt warmed index, release build, embedding excluded;
- 100 warmups and 1,000 samples;
- exact, internal BM25, and weighted hybrid;
- baseline and candidate binaries built once and run in interleaved order on
  the same host/toolchain; and
- median P95 across at least three final runs.

Each candidate median P95 MUST be no more than 3% above the pre-graph baseline,
with identical results and no graph initialization, files, or dispatch in the
graph-free process.

## 10. External comparison fairness

External comparisons use two lanes and publish source, lockfiles, build/run
instructions, raw results, and failures.

### 10.1 Engine isolation

Exact VectorKit is compared only with competent exact search using identical
precomputed corpus/query embeddings, canonical chunks, queries, metric,
normalization, filters, top K, hardware, and process conditions. Scalar and
Accelerate/vDSP exact scans and a supported embedded brute-force engine are
eligible.

ANN is a separate comparison against exact F32 ground truth. Tune ANN only on
development data, then compare latency at a locked matched Recall@10 target of
at least `0.99` unless a different threshold is approved before test. An ANN
row without achieved recall is invalid. Unconstrained ANN latency MUST NOT be
compared with exact latency.

Every row discloses build time, warm/cold query latency, peak memory, persisted
index size, source vector size, filter behavior/selectivity, update/deletion
support, save/load/validation behavior, and all search parameters.

### 10.2 Complete application stack

The complete lane compares VectorKit with a published, competent application
stack implementing the same vector retrieval, lexical/fusion behavior, graph
schema and bounded traversal, metadata filtering/intersection, document
projection, hydration, deletion/update semantics, and generation-consistent
persistence. Unsupported operations are explicit failures in a feature matrix,
not omitted measurements.

Use identical corpus, embeddings, query inputs, labels, workloads, and final
metrics. Report quality, complete evidence, latency stages and total, memory,
component/store sizes, number of stores, reload consistency, failure behavior,
integration operations, and lines of integration code. Code size is developer
experience evidence, not a speed or quality metric.

Cloud services may appear only in a labeled architecture/feature table. A
cloud-versus-local latency, memory, privacy, or cost winner claim is prohibited
because network and server topology are not comparable to the offline device
workload.

## 11. Decisions and Phase 1 consequences

### 11.1 Resolved by this contract

- V3 uses separate canonical corpus, graph schema, query, embedding, judgment,
  evidence, path, exclusion, and transformation-manifest files.
- Evaluation documents are canonical records; chunk identity is the structured
  `(record_id, chunk_key)` pair and document dedup keeps the first ranked chunk.
- TREC scores are rank-derived; native raw scores live only in diagnostics.
- Deterministic quality artifacts contain no measured time or timestamp.
- Global exclusions never enter `queries.jsonl`; derived resolver failures stay
  in the whole-corpus population and are stored only as policy-lane
  exclusions. Run populations are the formal sets in section 5.1.
- Queries without positive qrels are global pre-freeze exclusions; empty scopes
  and truncated traversals remain valid scored outcomes; empty reduction is
  `undefined`; and stale selections invalidate the affected run while failing
  the overall publication gate.
- Alternative evidence sets use maximum set recall/completeness; explicit and
  derived seed lanes are never aggregated together.
- Derived seeds use a frozen non-LLM exact-alias resolver. Failures are frozen
  derived-lane exclusions with mandatory resolution-coverage reporting and no
  silent fallback; longest-match length is measured after normalization.
- Exact metric objects, macro/micro aggregates, run-configuration hashes,
  generation fingerprints, and JSON escaping are defined in sections 2, 4,
  and 5. Byte identity applies only to equal determinism contexts.
- C and G use I8 weighted hybrid with identical alpha/candidate limits; paired
  comparison populations are mandatory.
- iPhone 17 Pro Max is the headline device and Apple M1 Max is the development
  Mac.

### 11.2 Assumptions

- Public graph-quality adapters can map one upstream document to one canonical
  record. If an adapter cannot, it must define a stable document projection in
  a new collection version before Phase 2.
- The current production graph schema/query surface and its four truncation
  reasons remain the Phase 1 execution target.
- The selected public collection can provide document-level qrels and complete
  alternative evidence sets without redistributing prohibited raw data.
- Weighted-hybrid alpha and candidate counts are configuration values chosen on
  development data, not Phase 0 universal constants.

### 11.3 Recorded owner decisions

On 2026-07-15, the repository owner:

1. approved the complete-retrieval benchmark roadmap and this contract as the
   Phase 0 implementation sources of truth; and
2. selected iPhone 14 Pro Max with iOS 26 or later as the conservative physical
   device. Its runtime hardware identifier, RAM class, and vector capabilities
   remain measurements required by section 9.1 before publishing performance
   results.

HotpotQA versus 2WikiMultiHopQA remains a Phase 2 adapter decision, not an
unresolved Phase 0 contract decision.

### 11.4 Expected Phase 1 change surface

Phase 1 is expected to change only evaluation tooling and tests, principally:

- `crates/vectorkit-cli/src/quality.rs`;
- `crates/vectorkit-cli/src/quality/artifacts.rs`;
- new graph-aware modules under `crates/vectorkit-cli/src/quality/`;
- `scripts/quality/validate_trec_metrics.py` and focused independent graph
  metric/path validators under `scripts/quality/`;
- `benchmarks/retrieval-quality/README.md`; and
- a small checked-in V3 smoke fixture under `benchmarks/retrieval-quality/` or a
  new benchmark-only graph-quality directory.

Phase 1 MUST NOT change production crates to expose benchmark data, Swift or
Python wrappers, production public APIs, graph database scope, or download a
public dataset. The evaluator should translate V3 records/schema/embeddings
into existing capability-separated APIs and retain benchmark-only diagnostics
in the CLI.

## 12. Phase 0 acceptance checklist

The first independent dry-run failed on 2026-07-15. Both reviewers agreed on
cases A-J and the primary worked-example calculations, but found blocking
ambiguity in query populations, empty-scope metric status, native chunk
overfetch, artifact schemas and hash preimages, stale-selection invalidation
scope, floating/JSON determinism, and the empty-candidate example. The complete
evidence is in
`docs/product/reports/graph-retrieval-phase-0-independent-review.md`.
Two fresh isolated reviewers then rejected the first revision because its
zero-row rule, transformation/result/manifest schemas, invalid-run aggregate
status, selection ordering, alias provenance, dirty-source boundary,
quantization identity, and cross-context matching still permitted divergent
artifacts. Two further fresh reviewers rejected the second revision: both found
that the named A-J dry-run cases had never been normatively defined, and one
also found an open transformation DAG, reserved policy-ID collisions, cosine
normalization order, `matched_terms` semantics, invalid-reason attribution,
binary64 reduction order, and cross-context projection. This third focused
revision closes those findings as one coupled population/artifact/hash model.
Two new fresh isolated reviewers then independently passed it on 2026-07-16.
Both reproduced the exact 2,135-byte fixture and its SHA-256, every published
population hash, all 15 required runs and statuses, the closed schemas and hash
preimages, the invalidation and portability rules, and both section 5.7
calculations without a blocker. The Phase 0 gate is closed.

| Roadmap Phase 0 deliverable or exit criterion | Contract evidence | Status |
| --- | --- | --- |
| Approve roadmap as implementation source of truth | Preamble and recorded owner decision 1 | Approved 2026-07-15 |
| Versioned graph-evaluation collection schema | Sections 2 and 3 | Resolved |
| Deterministic run identifiers and artifact filenames | Section 4 | Resolved |
| Complete-evidence metric semantics | Sections 5.1 and 5.3 | Resolved |
| Candidate metric and reduction semantics | Sections 5.3 and 5.4 | Resolved |
| Path metric semantics | Sections 3.7 and 5.4 | Resolved |
| Truncation metric semantics | Section 5.5 | Resolved |
| Explicit graph-seed contract | Section 6.1 | Resolved |
| Derived graph-seed contract | Section 6.2 | Resolved |
| Headline iPhone | Section 9.1: iPhone 17 Pro Max | Resolved |
| Conservative older-device target | Section 9.1 and recorded owner decision 2 | iPhone 14 Pro Max, iOS 26+ |
| Two implementations identify the same valid queries | Sections 3.6-3.8, 5.1, 6.2, 7.2, 8, and the normative A-J fixture below define tasks, global/lane exclusions, exact populations, alias provenance, resolver coverage, and lock sequence | Passed independently twice on 2026-07-16 |
| Two implementations calculate the same metrics and artifacts | Sections 2, 3.8, 4, 5, and the fixture below define zero-row bytes, exact escaping/arithmetic, stage ownership, normalization/BM25 traces, projection exhaustion, exact manifests/results, invalid-run attribution, hashes, portability projection, and both section 5.7 examples | Passed independently twice on 2026-07-16 |

### 12.1 Normative A-J dry-run fixture

Cases A-J are the following exact logical fixture, not run letters and not an
invitation to invent scenarios. The bytes between the fence are ten canonical
JSONL rows in case order, with one LF after case J and no blank line. Their
length is 2,135 bytes and SHA-256 is
`4d7b920b8ae591f0c05cd41abbc36c50210bbf23e6bfa0e09b4eebbffdea4f46`.
This Phase 0 fixture tests population/status logic only; it does not replace the
Phase 1 checked-in end-to-end collection fixture.

```jsonl
{"case_id":"A","derived_policy":null,"derived_resolution":null,"evidence_judgment":false,"expected_path_lanes":[],"explicit_seed":false,"global_exclusion":null,"query_id":"qa","tasks":["retrieval"]}
{"case_id":"B","derived_policy":null,"derived_resolution":null,"evidence_judgment":false,"expected_path_lanes":[],"explicit_seed":true,"global_exclusion":null,"query_id":"qb","tasks":["retrieval"]}
{"case_id":"C","derived_policy":null,"derived_resolution":null,"evidence_judgment":true,"expected_path_lanes":["explicit"],"explicit_seed":true,"global_exclusion":null,"query_id":"qc","tasks":["evidence","path"]}
{"case_id":"D","derived_policy":"topic","derived_resolution":"success","evidence_judgment":true,"expected_path_lanes":[],"explicit_seed":false,"global_exclusion":null,"query_id":"qd","tasks":["evidence","retrieval"]}
{"case_id":"E","derived_policy":"topic","derived_resolution":"success","evidence_judgment":true,"expected_path_lanes":["topic"],"explicit_seed":false,"global_exclusion":null,"query_id":"qe","tasks":["evidence","path"]}
{"case_id":"F","derived_policy":"topic","derived_resolution":"no_match","evidence_judgment":false,"expected_path_lanes":[],"explicit_seed":false,"global_exclusion":null,"query_id":"qf","tasks":["retrieval"]}
{"case_id":"G","derived_policy":"topic","derived_resolution":"ambiguous","evidence_judgment":false,"expected_path_lanes":[],"explicit_seed":false,"global_exclusion":null,"query_id":"qg","tasks":["retrieval"]}
{"case_id":"H","derived_policy":"topic","derived_resolution":"success","evidence_judgment":true,"expected_path_lanes":["explicit","topic"],"explicit_seed":true,"global_exclusion":null,"query_id":"qh","tasks":["evidence","path","retrieval"]}
{"case_id":"I","derived_policy":"team","derived_resolution":"success","evidence_judgment":false,"expected_path_lanes":[],"explicit_seed":false,"global_exclusion":null,"query_id":"qi","tasks":["path","retrieval"]}
{"case_id":"J","derived_policy":null,"derived_resolution":null,"evidence_judgment":false,"expected_path_lanes":[],"explicit_seed":false,"global_exclusion":"no_relevant_documents","query_id":"qj","tasks":["retrieval"]}
```

Rows with null `global_exclusion` map to `queries.jsonl`; case J exists only as
the global exclusion row. `explicit_seed:true` means membership in `X_exp`.
`derived_policy` means membership in that `X_p`; `no_match` and `ambiguous`
map to the two exact derived exclusion reasons and `F_p`; `success` maps to
`S_p`. `evidence_judgment` and `expected_path_lanes` declare the required
judgment rows. Every included retrieval case has a positive qrel. These rules
produce the following required sets and population hashes:

| Set | Ordered IDs | SHA-256 population hash |
| --- | --- | --- |
| `Q` | `qa qb qc qd qe qf qg qh qi` | `91be2f127eff88b3d41229df2904cb3b7203992673711e3ee960ade05c35496d` |
| `R` | `qa qb qd qf qg qh qi` | `c373605c9580a90c0194ed28f5e07debfef5f8315547e9af5eb2cae963bfd4e3` |
| `X_exp = S_exp` | `qb qc qh` | `533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5` |
| `X_topic` | `qd qe qf qg qh` | `a3b85dfbb4d7e5178e8cf34ab7c8d1474fbc03ceba933c731fbb83da012ad2f8` |
| `F_topic` | `qf qg` | `f1a82a3707574638a0dff6e16db2616c73c0692bcee0e55a21b565097d3267fb` |
| `S_topic` | `qd qe qh` | `be40e5a59829766e4ec9bc36e50f69f2c3f0b8c4f0e59fff0f253878622bac59` |
| `X_team = S_team` | `qi` | `1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d` |
| `F_team` | empty | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `X_exp intersect R` | `qb qh` | `2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f` |
| `X_topic intersect R` | `qd qf qg qh` | `d9bd478b70d090c4b9543d346a42f300977480baf6f7d65f1c30e3608153a082` |
| `S_topic intersect R` | `qd qh` | `b64c45f1a2bef306eb3daca23aaa916bcbc151fef367325a7160e9520651f24e` |
| `X_team intersect R = S_team intersect R` | `qi` | `1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d` |

The fixture therefore requires A-C; D for `explicit`, `topic`, and `team`; and
E-G separately for those same three lanes. Cases F/G are valid in A-C but
`excluded_pre_freeze` in every derived-`topic` D-G run. C appears only in D
explicit, E only in D topic, H independently in both explicit and topic lanes,
and I has `path_accuracy:not_applicable` because its team lane has no expected
path row. J appears in no run. Reviewers MUST report these exact memberships,
hashes, and statuses before checking the closed object shapes and hash
preimages in sections 2-6.

The independent dry-run is a specification-clarity review, not human relevance
judging. Two fresh reviewers independently apply the written validity rules,
use only the normative fixture in section 12.1, reconstruct
declared/execution populations and artifact objects, and calculate both section
5.7 variants without consulting one another or any prior review report. The coordinator
compares statuses, population hashes, object shapes, hash preimages, and metric
values within `1e-12`. Agreement shows that Phase 1 authors can implement the
contract without inventing an unstated policy.

Phase 0 exited on 2026-07-16 after two fresh independent implementation authors
completed that dry run and agreed. Both owner decisions are recorded. Phase 1
begins with the checked-in automated conformance fixture, evaluator, and
byte-comparison tests; production APIs and wrappers remain out of scope.
