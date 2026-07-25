//! Typed JNI boundary for the repository-local Kotlin/JVM artifacts.
//!
//! The implementation deliberately keeps handles private and moves retrieval,
//! filtering, graph traversal, projection, and persistence into Rust.

#![deny(unsafe_op_in_unsafe_fn)]

mod base;
#[cfg(feature = "graph")]
mod graph;
