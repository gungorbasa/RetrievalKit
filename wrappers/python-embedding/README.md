# retrievalkit-embedding

Optional local FP32 text embeddings for RetrievalKit's Python API. This package
is deliberately separate from the retrieval database distribution: constructing
or prefetching an `OnnxEmbedder` may download verified model artifacts, while
database initialization, indexing, search, and embedding inference never do.

```python
from retrievalkit_embedding import OnnxEmbedder

OnnxEmbedder.prefetch()
embedder = OnnxEmbedder.load(local_only=True)
vector = embedder.embed("local retrieval")
assert len(vector) == 384
```

The default and only production-exposed model profile is the pinned FP32
`all-MiniLM-L6-v2` export. Inputs use the qualified 256-token limit and every
result contains exactly 384 finite, L2-normalized Python floats. This model
precision is independent of RetrievalKit's default signed-I8 database storage.

Model artifacts are acquired only over verified HTTPS by the shared Rust
`ModelStore`. Use `cache_directory=` to select an application cache and
`local_only=True` to prohibit network access. Acquisition is visibly bounded to
`load(...)` and `prefetch(...)`; direct construction remains an equivalent
convenience for compatibility.

The official ONNX Runtime 1.24.3 dynamic library must be supplied through
`runtime_library_path=`, `RETRIEVALKIT_ONNX_RUNTIME_LIBRARY`, or the prepared
package-local runtime directory. Package-local discovery accepts only the
qualified macOS arm64 library with its pinned exact size and SHA-256. This
repository does not contain the runtime binary.

This distribution is included in the v0.1.0 release inventory but is not
available from PyPI until the protected release gates pass.

## Qualification

The opt-in `qualify.py` utility is excluded from normal offline tests. It can
emit model metadata and vectors for the shared conformance validator:

```console
python qualify.py conformance --input texts.json --output python-fp32.json \
  --local-only
```

It also measures cached construction, the first public inference call, and 50
warm-ups followed by 750 measured 32-token single-text embeddings:

```console
python qualify.py benchmark --local-only
```

Both commands require an installed extension, a qualified ONNX Runtime, and a
downloaded cache when `--local-only` is selected. They never publish artifacts
or packages.
