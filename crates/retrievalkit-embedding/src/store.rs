use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use fs2::FileExt;
use sha2::{Digest, Sha256};

use crate::{
    error::io_error, model::pinned_manifests, DownloadPolicy, EmbeddingError, EmbeddingProfile,
    ModelArtifact, ModelManifest, Result,
};

/// Verified local files required to construct an embedder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelFiles {
    pub manifest: ModelManifest,
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub common_file_paths: Vec<PathBuf>,
    pub repository_manifest_path: Option<PathBuf>,
}

/// Resolves pinned model artifacts into a verified local cache.
#[derive(Clone, Debug)]
pub struct ModelStore {
    cache_dir: PathBuf,
    download_policy: DownloadPolicy,
    manifests: Vec<ModelManifest>,
}

impl ModelStore {
    /// Uses the platform cache and RetrievalKit's built-in pinned manifests.
    pub fn new(download_policy: DownloadPolicy) -> Result<Self> {
        Self::with_cache_dir(default_cache_dir()?, download_policy)
    }

    /// Overrides the cache directory while retaining built-in pinned manifests.
    pub fn with_cache_dir(
        cache_dir: impl Into<PathBuf>,
        download_policy: DownloadPolicy,
    ) -> Result<Self> {
        Self::with_manifests(cache_dir, download_policy, pinned_manifests())
    }

    /// Creates a store with an explicit pinned manifest set.
    ///
    /// This is useful for an application-managed mirror and for network-free
    /// test fixtures. Artifact URLs still must use HTTPS.
    pub fn with_manifests(
        cache_dir: impl Into<PathBuf>,
        download_policy: DownloadPolicy,
        manifests: Vec<ModelManifest>,
    ) -> Result<Self> {
        if manifests.is_empty() {
            return Err(EmbeddingError::InvalidManifest(
                "at least one model profile is required".into(),
            ));
        }
        for manifest in &manifests {
            manifest.model.validate()?;
            manifest.tokenizer.validate()?;
            for artifact in &manifest.common_files {
                artifact.validate()?;
            }
            if let Some(artifact) = &manifest.repository_manifest {
                artifact.validate()?;
            }
        }
        Ok(Self {
            cache_dir: cache_dir.into(),
            download_policy,
            manifests,
        })
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn download_policy(&self) -> DownloadPolicy {
        self.download_policy
    }

    /// Verifies and returns one selected profile, downloading only during this call.
    pub fn selected(&self, profile: EmbeddingProfile) -> Result<ModelFiles> {
        let manifest = self
            .manifests
            .iter()
            .find(|manifest| manifest.info.profile == profile)
            .ok_or_else(|| {
                EmbeddingError::InvalidManifest(format!("profile {profile} is not available"))
            })?
            .clone();
        self.resolve(manifest)
    }

    /// Verifies/downloads every manifest profile up front.
    pub fn prefetch_all(&self) -> Result<Vec<ModelFiles>> {
        self.manifests
            .iter()
            .cloned()
            .map(|manifest| self.resolve(manifest))
            .collect()
    }

    fn resolve(&self, manifest: ModelManifest) -> Result<ModelFiles> {
        let model_dir = self
            .cache_dir
            .join(cache_component(&manifest.info.identifier))
            .join(&manifest.info.revision);
        fs::create_dir_all(&model_dir).map_err(|error| io_error(&model_dir, error))?;

        let lock_path = model_dir.join(".download.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| io_error(&lock_path, error))?;
        lock.lock_exclusive()
            .map_err(|error| io_error(&lock_path, error))?;

        let model_path = self.ensure_artifact(&model_dir, &manifest.model)?;
        let tokenizer_path = self.ensure_artifact(&model_dir, &manifest.tokenizer)?;
        let common_file_paths = manifest
            .common_files
            .iter()
            .map(|artifact| self.ensure_artifact(&model_dir, artifact))
            .collect::<Result<Vec<_>>>()?;
        let repository_manifest_path = manifest
            .repository_manifest
            .as_ref()
            .map(|artifact| self.ensure_artifact(&model_dir, artifact))
            .transpose()?;
        FileExt::unlock(&lock).map_err(|error| io_error(&lock_path, error))?;

        Ok(ModelFiles {
            manifest,
            model_path,
            tokenizer_path,
            common_file_paths,
            repository_manifest_path,
        })
    }

    fn ensure_artifact(&self, directory: &Path, artifact: &ModelArtifact) -> Result<PathBuf> {
        self.ensure_artifact_with(directory, artifact, download_verified)
    }

    fn ensure_artifact_with(
        &self,
        directory: &Path,
        artifact: &ModelArtifact,
        downloader: impl FnOnce(&ModelArtifact, &Path) -> Result<()>,
    ) -> Result<PathBuf> {
        let destination = directory.join(&artifact.relative_path);
        match validate_artifact(&destination, artifact) {
            Ok(true) => return Ok(destination),
            Ok(false) => {}
            Err(error) if self.download_policy == DownloadPolicy::LocalOnly => return Err(error),
            Err(_) => {
                fs::remove_file(&destination).map_err(|error| io_error(&destination, error))?;
            }
        }

        if self.download_policy == DownloadPolicy::LocalOnly {
            return Err(EmbeddingError::ModelUnavailable(destination));
        }

        downloader(artifact, &destination)?;
        if !validate_artifact(&destination, artifact)? {
            return Err(EmbeddingError::ModelUnavailable(destination));
        }
        Ok(destination)
    }
}

fn validate_artifact(path: &Path, artifact: &ModelArtifact) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let metadata = fs::metadata(path).map_err(|error| io_error(path, error))?;
    if metadata.len() != artifact.size {
        return Err(EmbeddingError::CorruptArtifact {
            path: path.to_path_buf(),
            expected_size: artifact.size,
            expected_sha256: artifact.sha256.clone(),
        });
    }
    let actual = sha256_file(path)?;
    if actual != artifact.sha256.to_ascii_lowercase() {
        return Err(EmbeddingError::CorruptArtifact {
            path: path.to_path_buf(),
            expected_size: artifact.size,
            expected_sha256: artifact.sha256.clone(),
        });
    }
    Ok(true)
}

fn download_verified(artifact: &ModelArtifact, destination: &Path) -> Result<()> {
    artifact.validate()?;
    let parent = destination.parent().ok_or_else(|| {
        EmbeddingError::InvalidManifest("artifact destination has no parent".into())
    })?;
    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".download-{}-{nonce}.tmp", std::process::id()));
    let result = (|| {
        let response =
            ureq::get(&artifact.url)
                .call()
                .map_err(|error| EmbeddingError::Download {
                    url: artifact.url.clone(),
                    message: error.to_string(),
                })?;
        let mut reader = response.into_body().into_reader();
        let mut file = File::create(&temporary).map_err(|error| io_error(&temporary, error))?;
        let copied =
            io::copy(&mut reader, &mut file).map_err(|error| EmbeddingError::Download {
                url: artifact.url.clone(),
                message: error.to_string(),
            })?;
        file.flush().map_err(|error| io_error(&temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error(&temporary, error))?;
        if copied != artifact.size {
            return Err(EmbeddingError::CorruptArtifact {
                path: temporary.clone(),
                expected_size: artifact.size,
                expected_sha256: artifact.sha256.clone(),
            });
        }
        let actual = sha256_file(&temporary)?;
        if actual != artifact.sha256.to_ascii_lowercase() {
            return Err(EmbeddingError::CorruptArtifact {
                path: temporary.clone(),
                expected_size: artifact.size,
                expected_sha256: artifact.sha256.clone(),
            });
        }
        fs::rename(&temporary, destination).map_err(|error| io_error(destination, error))?;
        sync_directory(parent)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|error| io_error(path, error))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| io_error(path, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error(path, error))?;
    }
    Ok(())
}

fn cache_component(identifier: &str) -> String {
    identifier
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn default_cache_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("RETRIEVALKIT_EMBEDDING_CACHE") {
        return Ok(PathBuf::from(path));
    }
    if cfg!(target_os = "windows") {
        return env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("RetrievalKit").join("embedding"))
            .ok_or_else(|| {
                EmbeddingError::InvalidManifest(
                    "LOCALAPPDATA is unset; provide a cache directory".into(),
                )
            });
    }
    if cfg!(target_os = "macos") {
        return home_dir()
            .map(|path| {
                path.join("Library")
                    .join("Caches")
                    .join("RetrievalKit")
                    .join("embedding")
            })
            .ok_or_else(|| {
                EmbeddingError::InvalidManifest("HOME is unset; provide a cache directory".into())
            });
    }
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(path).join("retrievalkit").join("embedding"));
    }
    home_dir()
        .map(|path| path.join(".cache").join("retrievalkit").join("embedding"))
        .ok_or_else(|| {
            EmbeddingError::InvalidManifest("HOME is unset; provide a cache directory".into())
        })
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EmbeddingModelInfo, ModelManifest};
    use std::fs;
    use tempfile::TempDir;

    fn fixture_manifest(root: &Path, profile: EmbeddingProfile) -> ModelManifest {
        let model_bytes = b"model fixture";
        let tokenizer_bytes = b"tokenizer fixture";
        let manifest = ModelManifest::new(
            EmbeddingModelInfo::new("fixture/model", "revision", profile, 384, 256, false).unwrap(),
            artifact(
                &format!("onnx/all-MiniLM-L6-v2-{}.onnx", profile.as_str()),
                model_bytes,
            ),
            artifact("tokenizer/tokenizer.json", tokenizer_bytes),
        )
        .unwrap();
        let directory = root.join("fixture_model").join("revision");
        let onnx_directory = directory.join("onnx");
        fs::create_dir_all(&onnx_directory).unwrap();
        fs::write(
            onnx_directory.join(format!("all-MiniLM-L6-v2-{}.onnx", profile.as_str())),
            model_bytes,
        )
        .unwrap();
        let tokenizer_directory = directory.join("tokenizer");
        fs::create_dir_all(&tokenizer_directory).unwrap();
        fs::write(tokenizer_directory.join("tokenizer.json"), tokenizer_bytes).unwrap();
        manifest
    }

    fn artifact(relative_path: &str, bytes: &[u8]) -> ModelArtifact {
        ModelArtifact::new(
            relative_path,
            format!("https://fixtures.invalid/{relative_path}"),
            format!("{:x}", Sha256::digest(bytes)),
            bytes.len() as u64,
        )
        .unwrap()
    }

    #[test]
    fn local_only_resolves_verified_fixture_without_network() {
        let directory = TempDir::new().unwrap();
        let manifest = fixture_manifest(directory.path(), EmbeddingProfile::Fp16);
        let store =
            ModelStore::with_manifests(directory.path(), DownloadPolicy::LocalOnly, vec![manifest])
                .unwrap();

        let files = store.selected(EmbeddingProfile::Fp16).unwrap();
        assert_eq!(fs::read(files.model_path).unwrap(), b"model fixture");
    }

    #[test]
    fn local_only_reports_corruption_without_deleting_it() {
        let directory = TempDir::new().unwrap();
        let manifest = fixture_manifest(directory.path(), EmbeddingProfile::Fp16);
        let model_path = directory
            .path()
            .join("fixture_model/revision/onnx/all-MiniLM-L6-v2-fp16.onnx");
        fs::write(&model_path, b"broken").unwrap();
        let store =
            ModelStore::with_manifests(directory.path(), DownloadPolicy::LocalOnly, vec![manifest])
                .unwrap();

        assert!(matches!(
            store.selected(EmbeddingProfile::Fp16),
            Err(EmbeddingError::CorruptArtifact { .. })
        ));
        assert_eq!(fs::read(model_path).unwrap(), b"broken");
    }

    #[test]
    fn prefetch_all_resolves_each_local_profile() {
        let directory = TempDir::new().unwrap();
        let manifests = EmbeddingProfile::ALL
            .into_iter()
            .map(|profile| fixture_manifest(directory.path(), profile))
            .collect();
        let store =
            ModelStore::with_manifests(directory.path(), DownloadPolicy::LocalOnly, manifests)
                .unwrap();
        assert_eq!(store.prefetch_all().unwrap().len(), 3);
    }

    #[test]
    fn download_policy_recovers_corrupt_artifact_with_mock_downloader() {
        let directory = TempDir::new().unwrap();
        let bytes = b"replacement model";
        let model_artifact = artifact("model.onnx", bytes);
        let destination = directory.path().join("model.onnx");
        fs::write(&destination, b"corrupt").unwrap();
        let manifest = ModelManifest::new(
            EmbeddingModelInfo::new(
                "fixture/model",
                "revision",
                EmbeddingProfile::Fp16,
                384,
                256,
                false,
            )
            .unwrap(),
            model_artifact.clone(),
            artifact("tokenizer.json", b"tokenizer"),
        )
        .unwrap();
        let store = ModelStore::with_manifests(
            directory.path(),
            DownloadPolicy::DownloadIfMissing,
            vec![manifest],
        )
        .unwrap();

        let resolved = store
            .ensure_artifact_with(directory.path(), &model_artifact, |_, path| {
                fs::write(path, bytes).map_err(|error| io_error(path, error))
            })
            .unwrap();

        assert_eq!(resolved, destination);
        assert_eq!(fs::read(resolved).unwrap(), bytes);
    }
}
