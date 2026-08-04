# BEIR Evaluation Adapters

> [RetrievalKit](../../../README.md) › Benchmarks › Retrieval quality › BEIR

Prepare SciFact or NFCorpus for external comparability without committing or
redistributing their corpora, queries, judgments, embeddings, or run outputs.

## Supported datasets

RetrievalKit supports the following evaluation-only BEIR collections:

| Dataset | Documents | Test queries | Test qrels | Archive MD5 |
| :-- | --: | --: | --: | :-- |
| SciFact | 5,183 | 300 | 339 | `5f7d1de60b170fc8027bb7898e2efca1` |
| NFCorpus | 3,633 | 323 | 12,334 | `a89dba18a62ef92f7d323ec890a0d38d` |

## Prepare a collection

Run `scripts/quality/prepare_beir.py` to download, verify, and prepare a
collection under `target/benchmarks/beir/`. No corpus, query, qrels, embedding,
or generated run artifact is checked into this directory.

## Licensing and redistribution

The source archives are provided through the
[BEIR dataset catalog](https://github.com/beir-cellar/beir/wiki/Datasets-available).
BEIR does not grant a blanket license for the underlying datasets. Users are
responsible for reviewing the SciFact and NFCorpus source licenses, retaining
required citations, and determining whether redistribution is permitted.

Dataset-specific terms:

- **SciFact:** the upstream project licenses claims and evidence annotations
  under CC BY 4.0. Corpus abstracts come from S2ORC and are licensed under
  ODC-By 1.0. Cite Wadden et al., “Fact or Fiction: Verifying Scientific
  Claims,” EMNLP 2020. See the
  [SciFact license](https://github.com/allenai/scifact/blob/master/LICENSE.md)
  and [project citation](https://github.com/allenai/scifact#citation).
- **NFCorpus:** the upstream terms permit academic use. Other uses of the
  included NutritionFacts.org data require consultation of its terms and
  contact with its author. Cite Boteva et al., “A Full-Text Learning to Rank
  Dataset for Medical Information Retrieval,” ECIR 2016. See the
  [NFCorpus terms and citation](https://www.cl.uni-heidelberg.de/statnlpgroup/nfcorpus/).

CI must not cache, publish, or redistribute the downloaded archives, extracted
collections, embeddings, or generated result artifacts. Any future CI
redistribution requires a separate license review and the required attribution;
the current adapters intentionally write only to ignored `target/` paths.

## Canonical comparison

Canonical comparison uses one RetrievalKit chunk per BEIR document and embeds the
title followed by two newlines and the document text. Alternative chunking is a
separate experiment and must not be compared directly with canonical BEIR
results.
