//! Lazy image provisioning — inspect → pull → build from embedded strings.

use crate::container::{
    embedded_image::{AGENT_DOCKERFILE, AGENT_ENTRYPOINT},
    types::{ContainerError, DockerCapability},
};

/// Default image name for the agent container.
const DEFAULT_IMAGE_NAME: &str = "lightarchitects/agent:latest";

/// Manages the agent container image lifecycle.
///
/// On startup the image may not exist. [`ImageManager::ensure_image`] checks
/// once (per process lifetime) and either:
/// 1. Returns immediately if the image is already present.
/// 2. Attempts `docker pull` from a registry.
/// 3. Builds from the embedded Dockerfile and entrypoint strings (slow path,
///    first time only).
#[derive(Clone)]
pub struct ImageManager {
    image_name: String,
    capability: DockerCapability,
}

impl ImageManager {
    /// Create a new image manager for the given Docker capability.
    ///
    /// If Docker is unavailable, [`ensure_image`](Self::ensure_image) is a no-op.
    #[must_use]
    pub fn new(capability: DockerCapability) -> Self {
        let image_name =
            std::env::var("LA_AGENT_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE_NAME.to_owned());
        Self {
            image_name,
            capability,
        }
    }

    /// Ensure the agent image exists locally.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::DockerUnavailable`] if Docker is not ready.
    /// Returns [`ContainerError::ImageBuildFailed`] if the embedded build fails.
    pub async fn ensure_image(&self) -> Result<(), ContainerError> {
        if self.capability != DockerCapability::Ready {
            return Err(ContainerError::DockerUnavailable);
        }

        // 1. Check if image exists locally
        let exists = tokio::process::Command::new("docker")
            .args(["image", "inspect", &self.image_name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .is_ok_and(|s| s.success());

        if exists {
            tracing::debug!(target: "container", image = %self.image_name, "image already present");
            return Ok(());
        }

        // 2. Try pull from registry (fast path)
        tracing::info!(target: "container", image = %self.image_name, "attempting docker pull");
        let pull = tokio::process::Command::new("docker")
            .args(["pull", &self.image_name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;

        match pull {
            Ok(st) if st.success() => {
                tracing::info!(target: "container", image = %self.image_name, "pull succeeded");
                return Ok(());
            }
            Ok(st) => {
                tracing::warn!(target: "container", image = %self.image_name, status = ?st, "pull failed — falling back to embedded build");
            }
            Err(e) => {
                tracing::warn!(target: "container", image = %self.image_name, error = %e, "pull errored — falling back to embedded build");
            }
        }

        // 3. Build from embedded Dockerfile (slow path, first time only)
        self.build_from_embedded().await
    }

    async fn build_from_embedded(&self) -> Result<(), ContainerError> {
        let tmp = std::env::temp_dir().join(format!("la-agent-build-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp)?;

        // Write embedded strings to temp dir
        std::fs::write(tmp.join("Dockerfile"), AGENT_DOCKERFILE)?;
        std::fs::write(tmp.join("agent-entrypoint.sh"), AGENT_ENTRYPOINT)?;

        // Copy binaries from host deploy path into build context
        let host_bin = lightarchitects::core::paths::root().map_or_else(
            || {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default()
                    .join(".lightarchitects")
                    .join("bin")
            },
            |r| r.join("bin"),
        );

        let gateway_bin = host_bin.join("lightarchitects");
        let cli_bin = host_bin.join("lightarchitects-cli");

        if gateway_bin.is_file() {
            std::fs::copy(&gateway_bin, tmp.join("lightarchitects"))?;
        } else {
            tracing::warn!(path = %gateway_bin.display(), "gateway binary not found — image may be incomplete");
        }

        if cli_bin.is_file() {
            std::fs::copy(&cli_bin, tmp.join("lightarchitects-cli"))?;
        } else {
            tracing::warn!(path = %cli_bin.display(), "cli binary not found — image may be incomplete");
        }

        tracing::info!(target: "container", image = %self.image_name, path = %tmp.display(), "building image from embedded Dockerfile");

        let status = tokio::process::Command::new("docker")
            .args(["build", "-t", &self.image_name, tmp.to_str().unwrap_or(".")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await?;

        // Best-effort cleanup of temp dir
        let _ = std::fs::remove_dir_all(&tmp);

        if !status.success() {
            return Err(ContainerError::ImageBuildFailed(self.image_name.clone()));
        }

        tracing::info!(target: "container", image = %self.image_name, "embedded build succeeded");
        Ok(())
    }
}
