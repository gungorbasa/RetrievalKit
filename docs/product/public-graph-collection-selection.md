# Public Graph Collection Selection

Status: selected for Phase 2a on 2026-07-17

## Decision

VectorKit's first public graph-quality collection is **HotpotQA distractor
release V1.1/V1 plus the January 14, 2019 linked-abstract corpus**. Development
queries come from train V1.1 and the locked reporting population comes from the
publicly judged distractor dev V1 split. The per-query distractor contexts are
inspection inputs only; they are not the retrieval corpus.

2WikiMultiHopQA is deferred. Its graph structure is attractive, but the
official repository's Apache-2.0 file governs the code and does not state a
license for dataset contents. The two official data links are mutable Dropbox
objects without publisher-provided checksums or versioned release assets. That
combination fails the licensing and source-stability gates; this decision does
not infer dataset permission from the code license.

## Direct comparison

| Gate | HotpotQA | 2WikiMultiHopQA |
|---|---|---|
| Dataset-content license | Clear: project page says dataset and processed Wikipedia are CC BY-SA 4.0 | Blocked: no dataset-content license found in the official project repository or paper |
| Code license | Apache-2.0 | Apache-2.0 |
| Source stability | Versioned question filenames, immutable pinned conversion snapshot, dated corpus archive, publisher MD5 plus locally recorded SHA-256 | April 7 files are identifiable, but Dropbox URLs are mutable and the publisher supplies no checksums/releases |
| Public judged reporting split | Distractor dev V1 has answers and complete supporting facts; fullwiki test judgments are hidden | Dev has answers, supporting facts, and evidence triples; test judgments are hidden |
| Global-corpus feasibility | Official October 1, 2017 linked abstracts provide one shared corpus independent of per-query contexts | Official hyperlink corpus provides one shared corpus |
| Fixed under-50K corpus | Yes: salted source-query exact-title seeds plus at most 15 outgoing neighbors per sampled query, hard bound 48,000 | Technically feasible with the same label-blind family of rules |
| Qrels-leakage risk | Controlled by separate source/judgment parsers and a corpus-builder signature with no judgment input | Gold `entity_ids`, evidence triples, supporting facts, and per-query contexts create more accidental-selection paths |
| Graph independence | Directed hyperlinks come only from `text_with_links` | Directed mentions/ref IDs can come only from the hyperlink corpus |
| Evidence completeness | Exactly two supporting documents per train/dev query in the inspected release | Two or four supporting documents, plus evidence triples, in train/dev |
| Path evaluation | Hyperlink paths are defensible only when derived from the frozen graph; gold support order is not a path | Evidence triples provide richer labels, but cannot be graph-construction inputs |
| Exact-alias seed | Deterministic longest exact title substring of the question, with ambiguity rejection | Feasible from corpus titles/aliases |
| Explicit structured seed | No natural non-gold field; lane is unavailable in V1 | `entity_ids` are gold paragraph identifiers and therefore prohibited as seeds |
| Deterministic reproduction | Yes with frozen artifacts, checksums, canonical JSON, source-only sampling salt, and fixed tie-breaks | Transformation could be deterministic, but acquisition identity and permission are not yet defensible |
| A-G matrix | Compatible with the V3 whole-corpus/graph, F32/I8, semantic/hybrid matrix | Technically compatible |
| Benchmark credibility | Established human-authored multi-hop benchmark, public supporting evidence, fixed Wikipedia provenance | Strong multi-hop evidence design, conditional on license and immutable-source resolution |
| Adapter complexity | Moderate: two-pass linked-corpus scan and title-link normalization | Higher: 7.0 GB uncompressed corpus, mention/ref schema, alias tables, and evidence alternatives |

## Official source inventory and licensing

### HotpotQA

- Project and dataset terms: [HotpotQA](https://hotpotqa.github.io/).
- Official code and release links: [hotpotqa/hotpot](https://github.com/hotpotqa/hotpot), inspected at commit
  `3635853403a8735609ee997664e1528f4480762a`.
- Paper: Yang, Qi, Zhang, Bengio, Cohen, Salakhutdinov, and Manning,
  “HotpotQA: A Dataset for Diverse, Explainable Multi-hop Question Answering,”
  EMNLP 2018, [DOI 10.18653/v1/D18-1259](https://aclanthology.org/D18-1259/).
- Question release: `hotpot_train_v1.1.json` (566,426,227 bytes, SHA-256
  `26650cf50234ef5fb2e664ed70bbecdfd87815e6bffc257e068efea5cf7cd316`)
  and `hotpot_dev_distractor_v1.json` (46,320,117 bytes, SHA-256
  `4e9ecb5c8d3b719f624d66b60f8d56bf227f03914f5f0753d6fa1b359d7104ea`).
  The canonical URLs are under
  `https://curtis.ml.cmu.edu/datasets/hotpot/`; the publisher host timed out
  during this inspection. Counts were therefore independently inspected from
  the immutable `hotpotqa/hotpot_qa` conversion snapshot
  `1908d6afbbead072334abe2965f91bd2709910ab`, whose three exact Parquet
  artifacts are frozen in the adapter contract.
- Corpus release: [January 14, 2019 linked abstracts](https://hotpotqa.github.io/wiki-readme.html),
  `enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2`, based on
  English Wikipedia 2017-10-01; 1,553,565,403 bytes; publisher MD5
  `01edf64cd120ecc03a2745352779514c`; SHA-256
  `1acca1c5cc93c4890ea51091d2bad7c3ef6987aead127ab88728dc9e26555729`.
- Dataset and processed-Wikipedia content are CC BY-SA 4.0. Redistribution
  requires attribution, a license link, change indication, and ShareAlike for
  adaptations. The code is Apache-2.0. VectorKit may privately cache verified
  inputs, but repository policy forbids committing raw data. Public cache or
  redistribution must carry the CC BY-SA obligations and upstream attribution.

### 2WikiMultiHopQA

- Official repository and downloads:
  [Alab-NII/2wikimultihop](https://github.com/Alab-NII/2wikimultihop), inspected
  at commit `13800e5be57df1b4040b9b1588c6c811779e69e9`.
- Paper: Ho, Nguyen, Sugawara, and Aizawa, “Constructing A Multi-hop QA Dataset
  for Comprehensive Evaluation of Reasoning Steps,” COLING 2020,
  [DOI 10.18653/v1/2020.coling-main.580](https://aclanthology.org/2020.coling-main.580/).
- April 7 question release `data_ids_april7.zip`: 258,968,175 bytes; SHA-256
  `95df2bf56fdabe034e27aebc580e02264232203cf52552f9efe8a919e5529eef`;
  MD5 `cbbd9f09448eae46b172929292e06471`; official URL
  `https://www.dropbox.com/s/ms2m13252h6xubs/data_ids_april7.zip`. It contains
  `train.json` (707,810,660 bytes), `dev.json` (57,614,142), `test.json`
  (53,838,398), and `id_aliases.json` (17,501,406).
- April 7 hyperlink corpus `para_with_hyperlink.zip`: 1,900,740,270 bytes;
  SHA-256
  `a585bdc3c39425446e4b2701a5f7f30051cb6c100d179322055d53dc0a71a723`;
  MD5 `21ed32d980cd93a09e96ace91c219c8c`; official URL
  `https://www.dropbox.com/s/wlhw26kik59wbh8/para_with_hyperlink.zip`. It
  contains `para_with_hyperlink.jsonl` (7,023,046,781 uncompressed bytes).
- The repository code is Apache-2.0. No official statement was found that puts
  the dataset contents under Apache-2.0 or another license. Until the publisher
  supplies dataset terms, raw data must not be committed, placed in CI caches,
  or redistributed. Local inspection does not imply redistribution permission.

## Verified structure

HotpotQA distractor train contains 90,447 questions, 899,667 per-query context
paragraphs, 3,703,344 sentences, 215,684 supporting sentences, and 482,021
unique context titles. Distractor dev contains 7,405 questions, 73,700 context
paragraphs, 306,487 sentences, 18,005 supporting sentences, and 66,581 unique
context titles. Every inspected train/dev question has two supporting
documents. Across those splits there are 507,494 context titles and no repeated
question IDs; 1,821 titles have conflicting per-query context text. This
conflict is one reason those contexts are not a corpus. Fullwiki test does not
publish answers or supporting facts.

2Wiki train contains 167,454 questions, 1,674,540 per-query contexts, 5,177,500
sentences, 404,884 supporting sentences, 409,882 evidence triples, and 369,378
unique titles. Dev contains 12,576 questions, 125,760 contexts, 400,943
sentences, 30,687 supporting sentences, 31,120 evidence triples, and 54,957
unique titles. Test contains 12,576 questions and 125,760 contexts, but no
public answers, supporting facts, or evidence triples. The shared hyperlink
corpus contains 5,989,847 records/unique IDs, 5,989,845 unique titles,
23,245,689 sentences, and 35,724,975 mentions; two titles conflict and
6,194,218 mentions lack ref IDs.

Hotpot preserves string question `id` values and decimal Wikipedia abstract
`id` values as stable upstream identities. The linked corpus has 5,233,329
records, 5,230,693 normalized titles, and 2,619 normalized-title text
conflicts; duplicate-title resolution therefore requires the frozen numeric-ID
tie-break in the adapter contract. 2Wiki preserves question `_id`, hyperlink
corpus `id`, titles, `entity_ids`, supporting facts, and
`evidences`/`evidences_id`. Its per-query contexts have 398,354 unique titles
across all splits and no conflicting text, while the global corpus has no
duplicate IDs and two conflicting duplicate titles.

## Leakage analysis

The selected construction reads only upstream query ID, question text, type,
and difficulty before freezing the corpus. It cannot read answer, context,
supporting facts, qrels, or paths. The source-only query samples are selected by
a fixed salted SHA-256 ordering. Candidate seeds use exact normalized corpus
titles occurring in the question. The corpus is the selected seed records plus
bounded outgoing hyperlink targets from the global linked-abstract source.
Only after the frozen corpus object exists does a separate parser open
supporting facts and retain queries whose complete supporting-document set is
present.

Prohibited leakage paths are: choosing corpus documents from per-query
contexts; choosing a corpus size from retention results; using supporting
titles, sentence facts, answers, Hotpot type/level, or 2Wiki evidence/entity IDs
to choose records, edges, aliases, seeds, or conflict winners; tuning on the
reporting population; treating gold support order as a graph path; or using an
LLM/entity extractor to infer seeds. Tests enforce parser separation and the
corpus-builder input boundary.

## Operational estimate

The source scan is evaluation-only and intentionally simple. A clean machine
needs about 3.8 GB of compressed source downloads for both candidates during
research; the selected Hotpot-only input is about 1.9 GB including pinned query
artifacts. The Hotpot corpus planner performs two sequential scans of 15,517
compressed shards, peaked near 3.4 GB RSS on the inspection machine, and took
roughly 10 minutes per construction while other inspection work was active.
For the actual 12,670 chunks, raw 384-dimensional vector payloads are about
19.5 MB F32 or 4.9 MB I8 before manifests/index overhead. Based on the existing
single-item Core ML baseline, adapter embedding is provisionally budgeted at
one to three minutes; the implementation task must measure it and keep it
separate from retrieval. No quality, latency, device, or customer-facing claim
follows from this planning estimate or selection.

The exact transformation, expected corpus counts, hashes, population policy,
graph schema, seed handling, and embedding identity are normative in
`public-graph-collection-adapter-contract-v1.md`.
