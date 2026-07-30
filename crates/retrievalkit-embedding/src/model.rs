use std::fmt;

use crate::{
    EmbeddingError, Result, ARTIFACT_MANIFEST_SHA256, ARTIFACT_REPOSITORY,
    ARTIFACT_REPOSITORY_REVISION, EMBEDDING_DIMENSION, MAX_INPUT_TOKENS, MODEL_MANIFEST_FILENAME,
    SOURCE_MODEL_REVISION,
};

/// Precision/quantization profile for the pinned local embedding model.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum EmbeddingProfile {
    /// Full 32-bit floating-point weights and the canonical default.
    #[default]
    Fp32,
    /// Half-precision model weights.
    Fp16,
    /// Signed 8-bit quantized weights.
    Q8,
}

impl EmbeddingProfile {
    pub const ALL: [Self; 3] = [Self::Fp32, Self::Fp16, Self::Q8];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fp32 => "fp32",
            Self::Fp16 => "fp16",
            Self::Q8 => "q8",
        }
    }
}

impl fmt::Display for EmbeddingProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Whether construction may fetch a missing or corrupt model artifact.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DownloadPolicy {
    /// Download verified artifacts when the cache does not already contain them.
    #[default]
    DownloadIfMissing,
    /// Never use the network.
    LocalOnly,
}

/// Stable identity and output contract for an embedding model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingModelInfo {
    pub identifier: String,
    pub revision: String,
    pub profile: EmbeddingProfile,
    pub dimension: usize,
    pub max_input_tokens: usize,
    pub produces_normalized_embeddings: bool,
}

impl EmbeddingModelInfo {
    pub fn new(
        identifier: impl Into<String>,
        revision: impl Into<String>,
        profile: EmbeddingProfile,
        dimension: usize,
        max_input_tokens: usize,
        produces_normalized_embeddings: bool,
    ) -> Result<Self> {
        if dimension == 0 {
            return Err(EmbeddingError::InvalidManifest(
                "embedding dimension must be greater than zero".into(),
            ));
        }
        if max_input_tokens == 0 {
            return Err(EmbeddingError::InvalidManifest(
                "maximum input tokens must be greater than zero".into(),
            ));
        }
        Ok(Self {
            identifier: identifier.into(),
            revision: revision.into(),
            profile,
            dimension,
            max_input_tokens,
            produces_normalized_embeddings,
        })
    }
}

/// One immutable file in a model manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelArtifact {
    pub relative_path: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl ModelArtifact {
    pub fn new(
        relative_path: impl Into<String>,
        url: impl Into<String>,
        sha256: impl Into<String>,
        size: u64,
    ) -> Result<Self> {
        let artifact = Self {
            relative_path: relative_path.into(),
            url: url.into(),
            sha256: sha256.into(),
            size,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.relative_path.is_empty()
            || self.relative_path.starts_with('/')
            || self.relative_path.contains("..")
        {
            return Err(EmbeddingError::InvalidManifest(format!(
                "unsafe artifact path '{}'",
                self.relative_path
            )));
        }
        if !self.url.starts_with("https://") {
            return Err(EmbeddingError::InsecureUrl(self.url.clone()));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(EmbeddingError::InvalidManifest(format!(
                "artifact '{}' has an invalid SHA-256",
                self.relative_path
            )));
        }
        if self.size == 0 {
            return Err(EmbeddingError::InvalidManifest(format!(
                "artifact '{}' has zero size",
                self.relative_path
            )));
        }
        Ok(())
    }
}

/// A pinned, self-contained model profile manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelManifest {
    pub info: EmbeddingModelInfo,
    pub model: ModelArtifact,
    pub tokenizer: ModelArtifact,
    pub common_files: Vec<ModelArtifact>,
    pub repository_manifest: Option<ModelArtifact>,
}

impl ModelManifest {
    pub fn new(
        info: EmbeddingModelInfo,
        model: ModelArtifact,
        tokenizer: ModelArtifact,
    ) -> Result<Self> {
        model.validate()?;
        tokenizer.validate()?;
        Ok(Self {
            info,
            model,
            tokenizer,
            common_files: Vec::new(),
            repository_manifest: None,
        })
    }

    pub fn with_common_files(mut self, artifacts: Vec<ModelArtifact>) -> Result<Self> {
        for artifact in &artifacts {
            artifact.validate()?;
        }
        self.common_files = artifacts;
        Ok(self)
    }

    pub fn with_repository_manifest(mut self, artifact: ModelArtifact) -> Result<Self> {
        artifact.validate()?;
        self.repository_manifest = Some(artifact);
        Ok(self)
    }
}

pub(crate) fn pinned_manifests() -> Vec<ModelManifest> {
    let base = format!(
        "https://huggingface.co/{ARTIFACT_REPOSITORY}/resolve/{ARTIFACT_REPOSITORY_REVISION}"
    );
    const TOKENIZER_SHA: &str = "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037";

    [
        (
            EmbeddingProfile::Fp32,
            "onnx/all-MiniLM-L6-v2-fp32.onnx",
            "beaa83a6670eb0ddae4d7c6f7a89acf69ed5d1fd747b083fa6f9f0145b2ee891",
            90_396_663,
        ),
        (
            EmbeddingProfile::Fp16,
            "onnx/all-MiniLM-L6-v2-fp16.onnx",
            "105482078caa44c0b57a70545207feb6b1a27bd36353a5cbeb6f2577eb409675",
            45_317_052,
        ),
        (
            EmbeddingProfile::Q8,
            "onnx/all-MiniLM-L6-v2-q8.onnx",
            "0017d61f7a597949b62c14cec764bc971f5b451483597686b6a304920f3a9250",
            30_040_323,
        ),
    ]
    .into_iter()
    .map(|(profile, local_path, sha256, size)| {
        let info = EmbeddingModelInfo::new(
            "sentence-transformers/all-MiniLM-L6-v2",
            SOURCE_MODEL_REVISION,
            profile,
            EMBEDDING_DIMENSION,
            MAX_INPUT_TOKENS,
            true,
        )
        .expect("built-in model info is valid");
        ModelManifest::new(
            info,
            ModelArtifact::new(
                local_path,
                format!("{base}/{local_path}?download=true"),
                sha256,
                size,
            )
            .expect("built-in model artifact is valid"),
            ModelArtifact::new(
                "tokenizer/tokenizer.json",
                format!("{base}/tokenizer/tokenizer.json?download=true"),
                TOKENIZER_SHA,
                466_247,
            )
            .expect("built-in tokenizer artifact is valid"),
        )
        .expect("built-in manifest is valid")
        .with_common_files(vec![
            ModelArtifact::new(
                "tokenizer/tokenizer_config.json",
                format!("{base}/tokenizer/tokenizer_config.json?download=true"),
                "acb92769e8195aabd29b7b2137a9e6d6e25c476a4f15aa4355c233426c61576b",
                350,
            )
            .expect("built-in tokenizer configuration is valid"),
            ModelArtifact::new(
                "tokenizer/special_tokens_map.json",
                format!("{base}/tokenizer/special_tokens_map.json?download=true"),
                "303df45a03609e4ead04bc3dc1536d0ab19b5358db685b6f3da123d05ec200e3",
                112,
            )
            .expect("built-in special-token map is valid"),
            ModelArtifact::new(
                "tokenizer/vocab.txt",
                format!("{base}/tokenizer/vocab.txt?download=true"),
                "07eced375cec144d27c900241f3e339478dec958f92fddbc551f295c992038a3",
                231_508,
            )
            .expect("built-in vocabulary is valid"),
        ])
        .expect("built-in common tokenizer files are valid")
        .with_repository_manifest(
            ModelArtifact::new(
                MODEL_MANIFEST_FILENAME,
                format!("{base}/{MODEL_MANIFEST_FILENAME}?download=true"),
                ARTIFACT_MANIFEST_SHA256,
                4_797,
            )
            .expect("built-in repository manifest artifact is valid"),
        )
        .expect("built-in repository manifest is valid")
    })
    .collect()
}
