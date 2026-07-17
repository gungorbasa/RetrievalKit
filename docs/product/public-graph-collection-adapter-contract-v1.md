# HotpotQA Public Graph Collection Adapter Contract V1

Status: frozen Phase 2a contract

This document is normative for the first public graph-quality adapter. The
implementation task may fill in code, manifests, and generated outputs, but it
must not change any source identity, selection rule, count, hash, graph rule,
population rule, seed rule, or embedding rule below without publishing V2 of
this contract.

## 1. Collection identity and boundaries

- `collection_id`: `hotpotqa-linked-abstracts-graph-v1`
- Upstream question release: HotpotQA distractor train V1.1 and distractor dev
  V1.
- Upstream corpus release: January 14, 2019 processed linked abstracts from
  English Wikipedia 2017-10-01.
- Development source split: train V1.1.
- Locked reporting source split: distractor dev V1. It is the public judged
  reporting population; the hidden-judgment fullwiki test split is not used.
- One upstream abstract is one record and exactly one chunk. Per-query
  distractor contexts are never corpus records.
- This is evaluation-only. It does not modify a production crate, wrapper, or
  public API and it does not authorize Phase 3 runs or any product claim.

## 2. Pinned upstream inputs

All downloaded bytes live under
`target/benchmarks/public-collections/downloads/`, which is ignored by Git.
Every artifact is verified by byte count and SHA-256 before parsing. A mismatch
is fatal; partial downloads use a `.download` suffix and are never parsed.

| Input | Exact source | Bytes | SHA-256 |
|---|---|---:|---|
| Linked abstracts | `https://nlp.stanford.edu/projects/hotpotqa/enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2` | 1,553,565,403 | `1acca1c5cc93c4890ea51091d2bad7c3ef6987aead127ab88728dc9e26555729` |
| Train shard 0 | `https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/1908d6afbbead072334abe2965f91bd2709910ab/distractor/train-00000-of-00002.parquet?download=true` | 165,624,177 | `76d3bb3048a7cc73c1958107c0c5872a00d7e7d00c105b81e92f6769e7822e68` |
| Train shard 1 | `https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/1908d6afbbead072334abe2965f91bd2709910ab/distractor/train-00001-of-00002.parquet?download=true` | 166,162,479 | `713661628434fbb19fff7392e2e321e4ed107e3c7c7784d0690946e5f722763f` |
| Distractor dev | `https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/1908d6afbbead072334abe2965f91bd2709910ab/distractor/validation-00000-of-00001.parquet?download=true` | 27,452,575 | `c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6` |

The canonical publisher JSON origins remain
`https://curtis.ml.cmu.edu/datasets/hotpot/hotpot_train_v1.1.json`
(566,426,227 bytes, SHA-256
`26650cf50234ef5fb2e664ed70bbecdfd87815e6bffc257e068efea5cf7cd316`)
and `hotpot_dev_distractor_v1.json` at the same directory (46,320,117
bytes, SHA-256
`4e9ecb5c8d3b719f624d66b60f8d56bf227f03914f5f0753d6fa1b359d7104ea`).
The immutable Parquet snapshot is the V1 adapter's acquisition input because
the publisher host was unavailable during contract qualification. It must
continue to produce exactly 90,447 train and 7,405 dev rows.

The linked archive has publisher MD5
`01edf64cd120ecc03a2745352779514c`, extracts to exactly 15,517 `.bz2`
JSONL shards below one root and 157 directories (15,674 tar members total).
The compact canonical JSON array of archive-order objects
`{"name":...,"size":...,"type":"dir"|"file"}` has SHA-256
`e2c7b289c1ed0c7e11faabd9ef1b37bceeea1a997e3673657bdfee053c6450cf`.
The shards contain 5,233,329 rows with 5,230,693 normalized unique
titles and 2,619 conflicting normalized titles. Each row has exactly
`id`, `url`, `title`, `text`, `text_with_links`, `charoffset`, and
`charoffset_with_links`; `id` is decimal; `text` and `text_with_links` are
parallel string arrays. Character-offset fields are schema-validated but are
not construction inputs. The question schema is
exactly string `id`, `question`, `answer`, `type`, and `level`, a
`supporting_facts` struct of parallel title-string and int32-sentence-ID lists,
and a `context` struct of parallel title and sentence-list arrays. Unknown,
missing, or wrong-typed fields fail validation.

Licensing and attribution inputs are the
[HotpotQA project terms](https://hotpotqa.github.io/), the
[linked-abstract release notice](https://hotpotqa.github.io/wiki-readme.html),
the [Apache-2.0 code license](https://github.com/hotpotqa/hotpot/blob/master/LICENSE.txt),
the [CC BY-SA 4.0 text](https://creativecommons.org/licenses/by-sa/4.0/), and
the [EMNLP 2018 paper](https://aclanthology.org/D18-1259/). Generated notices
must attribute the HotpotQA authors and Wikipedia contributors, link CC BY-SA
4.0, identify VectorKit transformations, and apply ShareAlike where required.

## 3. Source-only sampling and corpus construction

### 3.1 Structural isolation

The corpus-builder entry point accepts only a sequence of `SourceQuery` values
and an abstract-corpus directory. `SourceQuery` contains upstream ID, split,
question text, type, and level. It has no answer, context, supporting fact,
qrel, entity, evidence, or path field. Judgment parsing is a different function
called only after a `FrozenCorpus` value has returned. The builder must never
open a question/judgment file itself. Tests inspect this signature and execute
the source parser with deliberately unusable gold-field objects.

### 3.2 Query sampling

Validate all source rows and reject duplicate IDs. Within each split order
queries by the unsigned bytewise tuple:

```text
(SHA256("vectorkit-hotpotqa-linked-abstracts-v1" || NUL || split || NUL || upstream_id), upstream_id UTF-8)
```

Take the first 2,000 train rows and first 1,000 distractor-dev rows. Type,
level, context, answer, and supporting facts do not affect this order.

### 3.3 Normalization and title candidates

`normalize(s)` is Unicode NFC, default Unicode case-fold, collapse every run of
Unicode whitespace to one U+0020, strip leading/trailing whitespace by that
collapse, then NFC again. Punctuation is preserved. A title is an exact alias
candidate only when its complete normalized string is a substring of the
normalized question and both ends occur at a transition between alphanumeric
and non-alphanumeric characters or at string ends. No stemming, fuzzy match,
entity extractor, LLM, answer, or gold title is allowed.

For each query retain candidates with the longest normalized title. Deduplicate
them by record ID. Exactly one is `resolved`; zero is `no_match`; more than one
is `ambiguous`. Candidate and diagnostic lists use ascending UTF-8 byte order.

### 3.4 Frozen universe

For every resolved query request its seed title and the first 15 distinct
outgoing hyperlink target aliases in ascending normalized UTF-8 byte order.
The hard pre-resolution bound is `(2,000 + 1,000) * (1 + 15) = 48,000`, below
the product's 50K chunk ceiling. Scan the global abstract corpus; retain rows
whose normalized title is requested. A missing link target is recorded and
omitted. If multiple rows normalize to the same title, choose the lowest
numeric Wikipedia page ID; record whether their text conflicts. Self-links are
omitted. This choice is label-blind and may not be revised after eligibility is
known.

Stable `record_id` is `hotpotqa:wiki:<decimal-upstream-id>`. Preserve upstream
ID and original title. Record text is the exact concatenation of the upstream
`text` sentence array in source order, with no separator or trimming. One
record produces one chunk with `chunk_key` `abstract`; the stable chunk
identity is `(record_id, "abstract")`. Embedding/display text is exact original
title, two LF bytes, then record text. Record order is ascending `record_id`
UTF-8 bytes. Outgoing target IDs are deduplicated and sorted by UTF-8 bytes.

The corpus-construction preimage is compact canonical JSON containing the
sample salt, neighbor limit, total source rows, source conflict count, exact
conflict policy, sorted missing/selected-conflict title lists, and every
retained record including IDs, title, text, and sorted outgoing IDs. Its
SHA-256 is
`a59dd4edc535abde55d27aa8262d64b99d7a25c05754cd0724fef5216c5204c6`.
The expected output is 12,670 records, 12,670 chunks, 43,737 directed link
edges, 1,776 missing requested aliases, and 59 selected conflicts. Any
different count or hash is fatal.

## 4. Graph construction

The production-compatible canonical graph schema is:

- record type `WikipediaArticle` maps to node type `Article`; queryable field
  `title`;
- relationship `LinksTo` goes from `Article` to `Article`, reads
  `outgoing_record_ids`, has cardinality `many`, `missing_target: omit_edge`,
  `duplicate_references: deduplicate`, `allow_self_edge: false`, and no inverse;
- chunk node type `Chunk`, ownership relationship `ContainsChunk`, inverse
  `PartOf`.

There is one `Article` node and one `Chunk` node per record. `ContainsChunk`
occurs once per record. `LinksTo` occurrences follow each source record's
sorted outgoing-ID array; `occurrence_ordinal` is its zero-based position after
deduplication. Source-record order is the corpus order. Edges are directed;
traversal may request incoming or outgoing direction under the V3 contract.
Aliases are the normalized retained-title to record-ID table; collision winners
are already frozen by section 3.4.

Every article, ownership edge, and link edge traces only to abstract-corpus
fields `id`, `title`, `text`, and `text_with_links`. The graph constructor does
not accept a judgment object. Answers, per-query contexts, qrels, supporting
facts, query types/levels, and reasoning paths are not graph inputs. A malformed
href is ignored with a counted diagnostic; non-decimal/duplicate IDs,
missing required fields, parallel-array mismatch, corpus-count mismatch, or
schema-hash mismatch is fatal.

## 5. Queries, judgments, and populations

Only after corpus freeze, parse each sampled row's answer and complete set of
`(supporting title, sentence ID)` facts. Preserve the exact upstream query ID.
A query is globally eligible when it has at least one supporting fact and every
distinct normalized supporting title resolves to a retained record. Otherwise
exclude it as `missing_complete_evidence` or `not_in_frozen_corpus`. Eligibility
must not trigger a corpus rebuild.

- Development: 603 of 2,000 sampled train queries; population SHA-256
  `1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f`.
- Locked reporting: 297 of 1,000 sampled public distractor-dev queries;
  population SHA-256
  `9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010`.
- Globally excluded counts are frozen in the construction report:
  1,397 development and 703 reporting, all `not_in_frozen_corpus`; no sampled
  judgment lacks complete evidence annotations.

Population SHA-256 is over eligible upstream IDs in ascending UTF-8 order,
encoded as exact `id || LF` bytes. A document qrel has grade 1 for every
distinct supporting-title record. The complete supporting-document set is one
required alternative; repeated supporting sentences do not create extra
document qrels. Sentence IDs are retained as evidence provenance and validated
against the upstream context during judgment parsing, but per-query context
text is not emitted as corpus text.

HotpotQA labels supporting documents, not a traversed hyperlink sequence.
Consequently V1 emits no gold `expected_paths`; inventing a shortest graph path
would measure the adapter against itself. Candidate/evidence coverage and
observed traversal paths remain available under V3, while path accuracy is
explicitly not applicable for this collection version. Query `tasks` are
exactly `["evidence","retrieval"]`; `category` is exact upstream
`type + ":" + level`; `metadata_filter` and `explicit_seed` are null; and
`derived_seed_policy_id` is `hotpotqa-exact-title-v1`. Every query freezes this
traversal before reporting data is used:

```json
{"limits":{"max_hops":2,"max_results":10000,"max_visited":100000,"max_working_bytes":67108864},"steps":[{"direction":"outgoing","max_hops":2,"min_hops":0,"relationship_type":"LinksTo"}]}
```

These are production-default resource limits with a collection-specific
two-hop bound; they are not selected from judgment retention or reporting
results. There are no query filters in V1. `top_k` is 10,
`evaluation_depth` is 100, and `relevance_threshold` is 1.

Development data is the only tuning population. The reporting query list and
population hash are frozen by Phase 2 implementation and must not be inspected
for configuration selection. Fullwiki test remains excluded because its public
file has no judgments.

## 6. Seed lanes

### 6.1 Explicit structured lane

HotpotQA has no natural, non-gold structured query seed. This lane is
`unavailable`, contains zero queries, and its empty population hash is
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
Supporting titles and per-query context titles must not be substituted.

### 6.2 Deterministic exact-alias-derived lane

Use the section 3.3 resolution already computed from source question text and
the global corpus. Provenance records upstream query ID, source question
SHA-256, normalization policy ID `nfc-casefold-whitespace-v1`, candidate IDs,
chosen record ID/title, and status. A `no_match` or `ambiguous` query remains in
whole-corpus runs but is excluded from the derived-seed graph lane; it never
falls back to a gold title, entity ID, fuzzy match, or LLM.

- Development derived-seed population: 599 of the 603 globally eligible
  development queries; hash
  `da343545fa764b44c5382f4a16c933dded7bd613ae6e12768b5c2772c6739582`;
  the other four are `derived_seed_ambiguous`.
- Reporting derived-seed population: 296 of the 297 globally eligible reporting
  queries; hash
  `93c252bd743e4084c7c50e9f7dee970af2977967a62c5717ba8edc000101a9d8`;
  the other query is `derived_seed_ambiguous`.

The population hash uses the same sorted `id || LF` rule. Seed resolution
counts across all 3,000 sampled source queries are 2,763 `resolved`, 235
`ambiguous`, and 2 `no_match`.

## 7. Frozen MiniLM baseline

Reuse `target/embedding-models/all-MiniLM-L6-v2`; do not download or convert a
different model silently.

- Model ID and pinned upstream revision:
  `sentence-transformers/all-MiniLM-L6-v2@c9745ed1d9f207416be6d2e6f8de32d1f16199bf`.
- Core ML `model.mlmodel` SHA-256:
  `bb7f068c83217c5f4a39b4bad4aa75525847803485b46b7c226454a7d8f5e2fe`.
- Core ML `weight.bin` SHA-256:
  `84cbd97f75e18368c9ba9566bb51614f8f7d56f659c171124bf4447cc2145bde`.
- Package `Manifest.json` SHA-256:
  `e016b09b0886f4716add9817fe1ba040a201681e27bae5f317a34bab30c39afa`.
- Conversion `metadata.json` SHA-256:
  `31367d7310f9d5adcc727bf8f52bfb0bc6c6b31512fa3d83b7d5224cddf59784`.
- Tokenizer: local BERT tokenizer, `tokenizer.json` SHA-256
  `da0e79933b9ed51798a3ae27893d3c5fa4a201126cef75586296df9b4d2c62a0`,
  `tokenizer_config.json` SHA-256
  `872b6936be955bc3aea75ed599264d865626d68feede7e58b01e378e6332bd74`.
- Inputs: `input_ids`, `attention_mask`, and `token_type_ids`; right padding;
  longest-first right truncation to 256 tokens; batch size exactly 1.
- Pooling: attention-mask mean pooling. Output: 384-dimensional IEEE-754 F32,
  L2-normalized when norm is nonzero. Query/passage prefixes are empty.
- Corpus input is section 3.4 title/LF/LF/text. Query input is exact upstream
  question text. Input order is canonical chunk/query order.

The local converted package is a checksum-addressed evaluation input. A clean
machine must materialize those exact six model/tokenizer/metadata files from
the project's private artifact cache or reproduce them from the pinned upstream
revision and then satisfy every checksum above. The adapter fails if the model
directory is absent or mismatched; it must never fall back to Hugging Face
`main`, reconvert with an unrecorded toolchain, or accept numerically close
files as the same baseline. The upstream model card declares Apache-2.0.

`corpus-embeddings.f32.jsonl` and `query-embeddings.f32.jsonl` follow V3 section
3.5: one compact canonical JSON object per row with stable IDs and 384
canonical shortest-round-trip F32 values, LF terminated. Runtime is the pinned
Core ML package on macOS with the same compute-unit selection used by the
existing VectorKit baseline. A clean reproduction must compare embedding-file
SHA-256; a different runtime result is a failure, not a new baseline.

I8 is derived only from those frozen normalized F32 values. Per vector compute
`max_abs` in F32; zero vectors use scale 0 and all-zero bytes; otherwise scale
is `max_abs / 127` and each value is encoded as value times reciprocal scale,
round-half-away-from-zero, clamp `[-128,127]`, signed I8. Dot accumulation is
exact signed I32 and score is F32 `i32_dot * query_scale * chunk_scale`. This is
the unchanged V3 symmetric-per-vector policy. The embedding manifest records
all input checksums, model/tokenizer identities and checksums, runtime, compute
units, sequence length, batching, pooling, normalization, input construction,
dimension, F32 output hashes, and the exact V3 quantization object/hash.

## 8. Evaluation-only outputs and validation

The implementation writes only under
`target/benchmarks/public-collections/hotpotqa-linked-abstracts-graph-v1/`.
`development/` and `test/` are separate exact V3 collection roots. The `test`
name is the V3 split vocabulary; its upstream source is the locked, publicly
judged HotpotQA distractor-dev split. Both declare
`corpus_id: hotpotqa-linked-abstracts-corpus-v1`; collection IDs are
`hotpotqa-linked-abstracts-graph-v1-development` and
`hotpotqa-linked-abstracts-graph-v1-test`, both at `collection_version: 1`.

```text
adapter-manifest.json
inspection.json
source-inventory.json
development/
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
test/
  ...the same exact V3 inventory...
```

No other file is allowed below either collection root. Each has 12,670 records
and chunks. Development has 603 queries, 1,206 grade-1 qrel rows, 603 evidence
rows, zero expected-path rows, and 1,401 exclusion rows: 1,397 global plus four
derived-lane ambiguity rows. Test has 297 queries, 594 qrel rows, 297 evidence
rows, zero expected-path rows, and 704 exclusion rows: 703 global plus one
derived-lane ambiguity row. The two `records.jsonl`, `graph-schema.json`, and
corpus-embedding files must be byte-identical. `expected-paths.jsonl` is
exactly zero bytes.

All JSON/JSONL uses the V3 compact canonical encoding, UTF-8 without BOM, LF,
and exactly one final LF; arrays use the stated semantic order. TSV uses V3
qrel ordering. Collection schema is exactly V3; transformation-manifest schema
is 1. The six exact V3 manifests list tool name/version, input IDs and SHA-256,
exact policy parameters/preimage hash, output relative paths/counts/SHA-256,
and diagnostics. `adapter-manifest.json` closes the three files at the adapter
root plus both closed V3 roots; it is not placed inside either collection.
Repeated clean builds must have identical bytes.

All source/count/schema/checksum, population, record/chunk, graph edge,
embedding dimension/finite/norm, qrel/evidence completeness, alias provenance,
manifest closure, and canonical-byte checks fail closed. Unknown input versions
or files fail. Cache hits are accepted only after full byte-count and checksum
verification. Partial files are discarded. Generated outputs are replaced only
after complete validation.

Raw archives, extracted shards, upstream query files, converted model files,
embeddings, and generated corpora must not be committed. The adapter code,
tests, this contract, compact license/attribution instructions, and source
checksums may be committed. CI may use a private checksum-keyed cache while
preserving license notices; no public raw-data cache is part of V1.

## 9. Clean-machine reproduction

From repository root on macOS with Python 3.14 and the existing pinned Core ML
model conversion available:

```sh
python3 -m venv target/benchmarks/public-collections/inspection-venv
target/benchmarks/public-collections/inspection-venv/bin/pip install pyarrow==25.0.0 ruff==0.12.4
target/benchmarks/public-collections/inspection-venv/bin/python scripts/quality/inspect_public_graph_collections.py --cache-dir target/benchmarks/public-collections verify-sources --download
mkdir -p target/benchmarks/public-collections/sources/hotpotqa-abstracts
tar -xf target/benchmarks/public-collections/downloads/enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2 -C target/benchmarks/public-collections/sources/hotpotqa-abstracts
target/benchmarks/public-collections/inspection-venv/bin/python scripts/quality/inspect_public_graph_collections.py --cache-dir target/benchmarks/public-collections inspect-hotpotqa --abstracts-dir target/benchmarks/public-collections/sources/hotpotqa-abstracts --output target/benchmarks/public-collections/inspection/hotpotqa.json
target/benchmarks/public-collections/inspection-venv/bin/python scripts/quality/build_hotpotqa_graph_collection.py --cache-dir target/benchmarks/public-collections --abstracts-dir target/benchmarks/public-collections/sources/hotpotqa-abstracts --model-dir target/embedding-models/all-MiniLM-L6-v2 --output target/benchmarks/public-collections/hotpotqa-linked-abstracts-graph-v1 --repeat-and-compare
```

The final command is the exact interface the Phase 2 implementation must add.
It emits the layout in section 8, embeds with the frozen local model, validates
both closed V3 manifests and the adapter manifest, rebuilds into a second
temporary output, and byte-compares the two roots before atomically publishing.
It must require explicit acceptance of
the CC BY-SA attribution notice before first download. This contract leaves no
selection, graph, seed, population, or embedding-policy decision to that task.
