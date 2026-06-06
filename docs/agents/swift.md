# Swift Agent Guidance

Swift is used for VectorKit's Apple platform wrappers and benchmark harnesses.

## Scope

- Keep Swift wrapper code thin. Retrieval logic belongs in the Rust core.
- Use Swift for idiomatic Apple API surfaces, lifecycle ownership, packaging,
  and device benchmark harnesses.
- Prefer C ABI bindings to Rust for early integration. Do not reimplement
  vector search, filtering, ranking, or persistence in Swift.

## API And Ownership

- Make ownership explicit around FFI pointers. Every Rust-allocated string or
  buffer exposed to Swift must have a matching free function.
- Keep benchmark harnesses deterministic by passing explicit JSON configs.
- Surface Rust errors as structured Swift errors or JSON error payloads.

## Tooling

- For SwiftPM harnesses, document any required Rust build step and library
  search path.
- Verify with `swift build` or `swift run` for the relevant package.
- Keep iOS packaging work separate from macOS command-line harnesses unless the
  requested change explicitly needs both.
