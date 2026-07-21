# Reproduce and validate Phase 6

The Phase 6 generator reads frozen local evidence and performs no network or device access. Acquire HotpotQA and the pinned MiniLM model from the primary sources in `licensing.json`, then reproduce Phase 3 according to `docs/product/reports/hotpotqa-phase-3-locked-reporting-report.md`. Use the accepted Phase 4b root produced under the frozen target-device contract and the checked-in Phase 5 artifact root.

```sh
python3 benchmarks/publication/generate_publication.py --repo . --phase3-root target/benchmarks/hotpotqa-phase-3b/locked-reporting --phase4-root target/phase4b/device-results-v3-02b8971 --phase5-root benchmarks/external-reference/artifacts/mac-comparison-v1 --output /tmp/phase6-a
python3 benchmarks/publication/validate_publication.py --repo . --phase3-root target/benchmarks/hotpotqa-phase-3b/locked-reporting --phase4-root target/phase4b/device-results-v3-02b8971 --phase5-root benchmarks/external-reference/artifacts/mac-comparison-v1 --root /tmp/phase6-a
```

Generate a second fresh root with the same command and compare it byte-for-byte. Do not substitute rejected Phase 4b evidence, USearch timing, graph application winner comparisons, or 100K partial captures. Exact evidence identities and source revisions are in `manifest.json` and `evidence-index.json`.
