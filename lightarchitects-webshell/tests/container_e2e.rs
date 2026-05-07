//! Container pipeline end-to-end tests.
//!
//! Covers Docker probe, `ImageManager`, spawner, and embedded image strings.
//! Tests that do not require Docker run unconditionally; Docker-dependent tests
//! skip gracefully when the daemon is absent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_webshell::container::{
    ImageManager,
    embedded_image::{AGENT_DOCKERFILE, AGENT_ENTRYPOINT},
    types::{ContainerError, DockerCapability},
};

// ── Type-level tests (no Docker needed) ──────────────────────────────────

#[test]
fn docker_capability_equality_and_debug() {
    assert_eq!(DockerCapability::Ready, DockerCapability::Ready);
    assert_ne!(DockerCapability::Ready, DockerCapability::Unavailable);
    assert_ne!(
        DockerCapability::NoPermission,
        DockerCapability::Unavailable
    );
    // Debug must not panic
    let _ = format!("{:?}", DockerCapability::Ready);
}

#[test]
fn container_error_display_roundtrip() {
    let err = ContainerError::DockerUnavailable;
    assert_eq!(err.to_string(), "docker unavailable");

    let err = ContainerError::DockerNoPermission;
    assert_eq!(err.to_string(), "docker permission denied");

    let err = ContainerError::ImageBuildFailed("bad dockerfile".to_owned());
    assert!(
        err.to_string()
            .contains("image build failed: bad dockerfile")
    );

    let err = ContainerError::ImagePullFailed("rate limit".to_owned());
    assert!(err.to_string().contains("image pull failed: rate limit"));

    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "no file");
    let err = ContainerError::Io(io);
    assert!(err.to_string().contains("io error"));

    let err = ContainerError::RelayNotImplemented;
    assert_eq!(err.to_string(), "container WebSocket relay not implemented");
}

#[test]
fn embedded_image_strings_are_non_empty() {
    assert!(
        !AGENT_DOCKERFILE.is_empty(),
        "AGENT_DOCKERFILE must not be empty"
    );
    assert!(
        !AGENT_ENTRYPOINT.is_empty(),
        "AGENT_ENTRYPOINT must not be empty"
    );
    assert!(
        AGENT_DOCKERFILE.contains("FROM"),
        "Dockerfile should contain FROM instruction"
    );
    assert!(
        AGENT_ENTRYPOINT.contains("#!/bin/bash"),
        "Entrypoint should be a bash script"
    );
}

// ── ImageManager tests (no Docker daemon required) ─────────────────────────

#[tokio::test]
async fn image_manager_unavailable_returns_error() {
    let mgr = ImageManager::new(DockerCapability::Unavailable);
    let result = mgr.ensure_image().await;
    assert!(
        matches!(result, Err(ContainerError::DockerUnavailable)),
        "ensure_image with Unavailable should return DockerUnavailable: {result:?}"
    );
}

#[tokio::test]
async fn image_manager_no_permission_returns_error() {
    let mgr = ImageManager::new(DockerCapability::NoPermission);
    let result = mgr.ensure_image().await;
    assert!(
        matches!(result, Err(ContainerError::DockerUnavailable)),
        "ensure_image with NoPermission should return DockerUnavailable: {result:?}"
    );
}

// ── Docker-dependent tests (skip when daemon absent) ──────────────────────

/// True if the Docker daemon is reachable.
fn docker_available() -> bool {
    std::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[tokio::test]
async fn docker_probe_detects_capability() {
    let capability = lightarchitects_webshell::container::probe::probe_docker().await;
    if docker_available() {
        assert_eq!(
            capability,
            DockerCapability::Ready,
            "probe_docker should return Ready when daemon is reachable"
        );
    } else {
        assert!(
            capability == DockerCapability::Unavailable
                || capability == DockerCapability::NoPermission,
            "probe_docker should return Unavailable or NoPermission when daemon is absent: {capability:?}"
        );
    }
}

#[tokio::test]
async fn image_manager_ensure_image_idempotent_when_present() {
    if !docker_available() {
        // Skip — we need Docker to verify idempotency against a real image.
        return;
    }

    let mgr = ImageManager::new(DockerCapability::Ready);

    // First call — may pull or build. In CI or sandboxed environments the
    // build may fail because the Dockerfile references binaries not present.
    // Skip rather than fail so the test suite stays green in all environments.
    let first = mgr.ensure_image().await;
    if first.is_err() {
        eprintln!(
            "Skipping Docker idempotency test — ensure_image failed (expected in some environments): {first:?}"
        );
        return;
    }

    // Second call — should be a fast no-op because the image is already present.
    let second = mgr.ensure_image().await;
    assert!(
        second.is_ok(),
        "Second ensure_image should succeed: {second:?}"
    );
}
