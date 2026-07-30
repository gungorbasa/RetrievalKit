//! Optional local text embeddings for RetrievalKit.
//!
//! This crate is deliberately separate from `retrievalkit-core`. Model
//! acquisition happens only while building [`OnnxTextEmbedder`]; embedding
//! calls are local and operate on the already loaded ONNX Runtime session.

mod embedder;
mod error;
mod model;
mod store;

pub use embedder::{OnnxTextEmbedder, OnnxTextEmbedderBuilder, TextEmbedder};
pub use error::{EmbeddingError, Result};
pub use model::{
    DownloadPolicy, EmbeddingModelInfo, EmbeddingProfile, ModelArtifact, ModelManifest,
};
pub use store::{ModelFiles, ModelStore};

/// Output width of the pinned MiniLM model family.
pub const EMBEDDING_DIMENSION: usize = 384;
/// Maximum sequence length accepted by the pinned MiniLM model family.
pub const MAX_INPUT_TOKENS: usize = 256;
/// Stable filename used by packaged/downloadable artifact manifests.
pub const MODEL_MANIFEST_FILENAME: &str = "manifest-v1.json";
/// Frozen source-model revision shared by every runtime export.
pub const SOURCE_MODEL_REVISION: &str = "c9745ed1d9f207416be6d2e6f8de32d1f16199bf";
/// Public repository containing RetrievalKit's generated runtime exports.
pub const ARTIFACT_REPOSITORY: &str = "gungorbasa/retrievalkit-minilm";
/// Immutable artifact-repository commit used by the built-in downloader.
pub const ARTIFACT_REPOSITORY_REVISION: &str = "617ce926c1f9e0289365d3e999474cc28b1645d4";
/// SHA-256 of the authoritative manifest at the pinned artifact revision.
pub const ARTIFACT_MANIFEST_SHA256: &str =
    "b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2";
/// Exact ONNX Runtime release required by the built-in provider experiment.
pub const ONNX_RUNTIME_VERSION: &str = "1.24.3";
/// Environment variable used when a builder does not receive a runtime path.
pub const ONNX_RUNTIME_LIBRARY_ENV: &str = "RETRIEVALKIT_ONNX_RUNTIME_LIBRARY";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fp32_is_the_default_profile() {
        assert_eq!(EmbeddingProfile::default(), EmbeddingProfile::Fp32);
    }

    #[test]
    fn download_if_missing_is_the_default_policy() {
        assert_eq!(DownloadPolicy::default(), DownloadPolicy::DownloadIfMissing);
    }

    #[test]
    fn pinned_artifact_layout_is_stable() {
        assert_eq!(MODEL_MANIFEST_FILENAME, "manifest-v1.json");
        assert_eq!(SOURCE_MODEL_REVISION.len(), 40);
        assert_eq!(ARTIFACT_REPOSITORY_REVISION.len(), 40);
        assert_eq!(ARTIFACT_MANIFEST_SHA256.len(), 64);
        let manifests = model::pinned_manifests();
        let model_paths: Vec<_> = manifests
            .iter()
            .map(|manifest| manifest.model.relative_path.as_str())
            .collect();
        assert_eq!(
            model_paths,
            [
                "onnx/all-MiniLM-L6-v2-fp32.onnx",
                "onnx/all-MiniLM-L6-v2-fp16.onnx",
                "onnx/all-MiniLM-L6-v2-q8.onnx",
            ]
        );
        assert!(manifests
            .iter()
            .all(|manifest| manifest.tokenizer.relative_path == "tokenizer/tokenizer.json"));
    }
}
