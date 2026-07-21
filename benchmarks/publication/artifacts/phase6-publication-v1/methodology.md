# VectorKit benchmark methodology

Report date: 2026-07-21. Claims expire: 2027-07-21.

## Evidence families

Retrieval quality uses the official HotpotQA test split transformed into 12,670 linked-abstract records/chunks, 297 declared queries and 594 qrels. One pre-frozen ambiguous seed is excluded from graph comparison, leaving 296 common queries. Embeddings are `sentence-transformers/all-MiniLM-L6-v2` at pinned revision `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`, 384 dimensions.

Mac systems results use 10K, 25K, and 50K synthetic 384-dimensional workloads, top-10 retrieval, 20 warmups, and 100 samples. VectorKit exact F32 is checked against the NumPy 2.5.1 oracle and compared with sqlite-vec 0.1.9 exact F32. USearch 2.26.0 is an ANN lane with a recall gate, not an exact-capability peer. The graph applications have non-equivalent hybrid semantics and are not ranked.

Physical-device results use the frozen 10K, 25K, and 50K Phase 4b workloads in F32 and I8 on an iPhone 17 Pro Max (`iPhone18,2`, `V54AP`). Query percentiles cover five fresh-process sessions of 1,000 samples after 100 warmups. Graph-free ratios use the median of three session P95 values. Query sessions and 10K F32 prepare evidence report iOS 26.5.1 (23F81); remaining 815 lifecycle artifacts report iOS 26.5.2 (23F84). This variance is preserved.

## Timing and calculations

Embedding is excluded everywhere. Timings cover retrieval/application work identified in the frozen contracts. Percentiles use nearest rank. Phase 4b published query values are medians of five per-session percentiles; Phase 5 values are direct percentiles over 100 samples. Display rounding uses decimal ROUND_HALF_UP: milliseconds to three decimals, ratios and percentages to two.

## Gates and failures

Exact lanes must pass identity, filtering, deletion, determinism, and reload gates. ANN timing is comparison-eligible only after its recall gate passes. Failed, partial, diagnostic, rejected, and disqualified evidence cannot support positive claims. Phase 5 acceptance failed solely because USearch missed Recall@10; its recall is retained as a negative result and its timing is excluded. Phase 4b supported-product and graph-free qualification passed. The 100K stress lane is `not_run_device_safety`, has zero accepted artifacts, and cannot support any product or marketing claim. No Phase 6 device command or retuning is permitted.

## Environments and source

Mac: Apple M1 Max arm64, 10 logical CPUs, macOS 26.5.2 / Darwin 25.5.0, CPython 3.12.12, rustc/cargo 1.92.0, and Apple Swift 6.3.3, with VectorKit revision `9c784d2f11b91bb907150aa1b6046880ff89fde6`. Device hardware and OS builds are stated above. Phase 3, 4b, and 5 source and artifact identities are bound by `manifest.json` and `evidence-index.json`.

## Reproduction and licensing

Use `reproduction.md` and the independent validator. Raw HotpotQA payloads, model weights, raw device captures, binaries, and rejected/disqualified evidence are excluded. `licensing.json` provides primary-source license references and decisions. The repository has no root project license, so this package is repository-local and external redistribution remains withheld.
