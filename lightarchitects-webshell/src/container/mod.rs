//! Docker auto-detection and transparent containerization.
//!
//! On startup the webshell probes Docker capability (socket + CLI + permission
//! check). If ready, the agent spawns in containers by default; if absent or
//! permission-denied, it falls back to the native `portable-pty` path.
//!
//! All container logic is self-contained — no `Dockerfile` in the project root.
//! The agent image is built from embedded string constants on first spawn.

pub(crate) mod docker_cmd;
pub mod embedded_image;
pub mod image_manager;
pub mod policy_routes;
pub mod probe;
pub mod spawner;
pub mod types;

pub use image_manager::ImageManager;
pub use types::{ContainerError, ContainerMode, DockerCapability};
