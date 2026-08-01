# Third-party notices

This package depends on:

- `onnxruntime-web` 1.27.0, licensed under MIT. Its complete license and
  generated third-party notices are distributed as
  `dist/runtime/ONNXRUNTIME-LICENSE` and
  `dist/runtime/ONNXRUNTIME-ThirdPartyNotices.txt`.
- `@huggingface/tokenizers` 0.1.3, licensed under Apache-2.0. Its complete
  license is distributed as `dist/runtime/HUGGINGFACE-TOKENIZERS-LICENSE`.

The build fails if the checked legal-file digests or the installed tokenizer
license drift. Runtime loader/WASM size and SHA-256 identities are verified
before they are copied into `dist/runtime/`.
