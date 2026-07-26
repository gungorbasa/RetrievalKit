# RetrievalKit Python Graph Source Preview

This archive is a deterministic export of the public RetrievalKit repository
revision shown on the documentation site. It contains the Rust workspace and
the graph-capable Python wrapper needed for one end-to-end macOS arm64 source
build. Registry packages remain unpublished.

## Prerequisites

- macOS on Apple silicon
- CPython 3.10–3.14 with `venv`
- Rust stable with `cargo`

From the extracted archive root, build and validate the wrapper:

```bash
PYTHON_BIN=python3 scripts/check-python-graph-wrapper.sh
```

Then run the checked-in graph-scoped hybrid retrieval example:

```bash
target/python-graph-wrapper-check-venv-py*/bin/python \
  wrappers/python-graph/examples/graph_retrieval_quickstart.py
```

Expected output begins with:

```text
graph-hybrid=decision-swift
graph-candidates=1/2
```

The example first traverses the `contains` relationship from Project Apollo,
then ranks only the selected, approved notes using vector and BM25 evidence.

For all language guides and the current release status, use the public
repository: <https://github.com/gungorbasa/RetrievalKit>.
