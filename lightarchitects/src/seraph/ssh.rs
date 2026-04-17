//! SSH transport for remote SERAPH instances (e.g. Khadas Edge 2 Pro).
//!
//! Feature-gated behind `feature = "ssh"`. Provides [`SshSession`] for
//! executing commands on a remote host, [`SshSessionBuilder`] for type-safe
//! construction, and [`KeyPassphraseProvider`] strategies for encrypted keys.
//!
//! # Design
//!
//! A fresh SSH connection is opened for every call (one-shot, stateless). This
//! trades latency for simplicity and avoids stale-session handling. The Khadas
//! ED2P LAN round-trip is ~0.3 ms so the overhead is acceptable for interactive
//! pentest use.
//!
//! This module uses the [`openssh`] crate, which delegates to the system `ssh`
//! binary and its ControlMaster multiplexing. Passphrase-protected keys are
//! handled transparently by `ssh-agent` or the macOS Keychain — there is no
//! inline passphrase injection hook. The [`KeyPassphraseProvider`] types remain
//! available as standalone utilities (e.g., for other credential workflows) but
//! are no longer wired into [`SshSession`] directly.
//!
//! # Key handling
//!
//! Private key material never passes through this crate at runtime. The system
//! `ssh` binary handles all key loading. The [`zeroize`] dependency is retained
//! for the [`KeyPassphraseProvider`] implementations, which zero passphrase
//! strings on drop when used in other contexts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use openssh::{KnownHosts, SessionBuilder, Stdio};
use tokio::io::AsyncWriteExt as _;
use tokio::runtime::Runtime;
use zeroize::Zeroizing;

use crate::core::error::SdkError;

// ── Passphrase providers ────────────────────────────────────────────────────

/// Strategy for retrieving a passphrase to decrypt a password-protected SSH key.
///
/// All passphrase material is returned inside [`Zeroizing<String>`] so the heap
/// bytes are zeroed on drop regardless of the caller's error path.
///
/// These providers are available as standalone utilities. Note that [`SshSession`]
/// does not accept a passphrase provider — the `openssh` crate delegates key
/// decryption to the system `ssh-agent` or macOS Keychain.
pub trait KeyPassphraseProvider: Send + Sync {
    /// Retrieve the passphrase.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError`] if the passphrase cannot be obtained.
    fn get_passphrase(&self) -> Result<Zeroizing<String>, SdkError>;
}

/// Reads the passphrase from an environment variable.
pub struct EnvPassphraseProvider {
    var_name: String,
}

impl EnvPassphraseProvider {
    /// Create a provider that reads from `var_name`.
    #[must_use]
    pub fn new(var_name: impl Into<String>) -> Self {
        Self {
            var_name: var_name.into(),
        }
    }
}

impl KeyPassphraseProvider for EnvPassphraseProvider {
    fn get_passphrase(&self) -> Result<Zeroizing<String>, SdkError> {
        let value = std::env::var(&self.var_name).map_err(|_| {
            SdkError::Config(format!(
                "passphrase env var '{}' is not set or not valid UTF-8",
                self.var_name
            ))
        })?;
        Ok(Zeroizing::new(value))
    }
}

/// Reads the passphrase from the first line of a file.
///
/// The line is trimmed of leading/trailing whitespace. The full file contents
/// are wrapped in `Zeroizing<String>` during read so they are zeroed on drop.
pub struct FilePassphraseProvider {
    path: PathBuf,
}

impl FilePassphraseProvider {
    /// Create a provider that reads from `path`.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl KeyPassphraseProvider for FilePassphraseProvider {
    fn get_passphrase(&self) -> Result<Zeroizing<String>, SdkError> {
        let raw = Zeroizing::new(std::fs::read_to_string(&self.path).map_err(|e| {
            SdkError::Config(format!(
                "failed to read passphrase file at {}: {e}",
                self.path.display()
            ))
        })?);
        // Take only the first line, trimmed.
        let first_line = raw.lines().next().unwrap_or("").trim();
        Ok(Zeroizing::new(first_line.to_owned()))
    }
}

/// Invokes a user-supplied closure to obtain the passphrase.
///
/// Useful for interactive prompts, keychain integrations, or vault lookups.
pub struct CallbackPassphraseProvider {
    callback: Box<dyn Fn() -> Result<String, SdkError> + Send + Sync>,
}

impl CallbackPassphraseProvider {
    /// Create a provider backed by `callback`.
    #[must_use]
    pub fn new(callback: Box<dyn Fn() -> Result<String, SdkError> + Send + Sync>) -> Self {
        Self { callback }
    }
}

impl KeyPassphraseProvider for CallbackPassphraseProvider {
    fn get_passphrase(&self) -> Result<Zeroizing<String>, SdkError> {
        let value = (self.callback)()?;
        Ok(Zeroizing::new(value))
    }
}

// ── SshSessionBuilder ───────────────────────────────────────────────────────

/// Type-safe builder for [`SshSession`].
///
/// Required fields: `host`, `user`, `key_path`. Port defaults to 22.
///
/// Passphrase-protected keys are decrypted by `ssh-agent` or the macOS Keychain
/// — no inline passphrase provider is needed or accepted.
///
/// # Example
///
/// ```no_run
/// use crate::seraph::ssh::SshSessionBuilder;
///
/// let session = SshSessionBuilder::new()
///     .host("10.129.155.20")
///     .user("khadas")
///     .key_path("/path/to/key")
///     .build()
///     .unwrap();
/// ```
pub struct SshSessionBuilder {
    host: Option<String>,
    port: u16,
    user: Option<String>,
    key_path: Option<PathBuf>,
}

impl Default for SshSessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SshSessionBuilder {
    /// Start building a new [`SshSession`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: None,
            port: 22,
            user: None,
            key_path: None,
        }
    }

    /// Set the remote host (required).
    #[must_use]
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Set the SSH port (defaults to 22).
    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set the remote username (required).
    #[must_use]
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Set the path to the SSH private key (required).
    #[must_use]
    pub fn key_path(mut self, path: impl AsRef<Path>) -> Self {
        self.key_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Consume the builder and produce an [`SshSession`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if a required field is missing or the
    /// tokio runtime cannot be created.
    pub fn build(self) -> Result<SshSession, SdkError> {
        let host = self
            .host
            .ok_or_else(|| SdkError::Config("SshSessionBuilder: 'host' is required".into()))?;
        let user = self
            .user
            .ok_or_else(|| SdkError::Config("SshSessionBuilder: 'user' is required".into()))?;
        let key_path = self
            .key_path
            .ok_or_else(|| SdkError::Config("SshSessionBuilder: 'key_path' is required".into()))?;
        let runtime = Runtime::new()
            .map_err(|e| SdkError::Config(format!("failed to build tokio runtime: {e}")))?;

        Ok(SshSession {
            host,
            port: self.port,
            user,
            key_path,
            runtime: Arc::new(runtime),
        })
    }
}

// ── SshSession ──────────────────────────────────────────────────────────────

/// Configuration for SSH-backed remote command execution against a SERAPH host.
///
/// Each call opens a fresh SSH connection via the system `ssh` binary (through
/// the [`openssh`] crate's `ControlMaster` multiplexing). The tokio `Runtime` is
/// shared via `Arc` across clone boundaries.
///
/// Passphrase-protected keys are handled transparently by `ssh-agent` or the
/// macOS Keychain. There is no inline passphrase injection — configure key
/// access through your SSH agent before calling [`run`](SshSession::run).
///
/// Server host keys are accepted on first use (`StrictHostKeyChecking=no`),
/// which is appropriate for the Khadas ED2P on a dedicated private LAN.
/// Production deployments on public networks should pin a fingerprint via
/// `~/.ssh/known_hosts` and use [`KnownHosts::Strict`].
pub struct SshSession {
    host: String,
    port: u16,
    user: String,
    key_path: PathBuf,
    runtime: Arc<Runtime>,
}

impl std::fmt::Debug for SshSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSession")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("key_path", &self.key_path)
            .finish_non_exhaustive()
    }
}

impl SshSession {
    /// Start a builder for constructing an [`SshSession`].
    #[must_use]
    pub fn builder() -> SshSessionBuilder {
        SshSessionBuilder::new()
    }

    /// Build an SSH session configuration (simple constructor).
    ///
    /// Does **not** open a connection at construction time.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if the tokio runtime cannot be created.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        key_path: impl AsRef<Path>,
    ) -> Result<Self, SdkError> {
        let runtime = Runtime::new()
            .map_err(|e| SdkError::Config(format!("failed to build tokio runtime: {e}")))?;
        Ok(Self {
            host: host.into(),
            port,
            user: user.into(),
            key_path: key_path.as_ref().to_path_buf(),
            runtime: Arc::new(runtime),
        })
    }

    /// Default connection parameters for the Khadas Edge 2 Pro on the dev LAN.
    ///
    /// Key path defaults to `~/.ssh/id_ed25519`.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if the tokio runtime cannot be created.
    pub fn khadas() -> Result<Self, SdkError> {
        let key_path = dirs_home()
            .unwrap_or_else(|| PathBuf::from("/root"))
            .join(".ssh")
            .join("id_ed25519");
        Self::new("10.129.155.20", 22, "khadas", key_path)
    }

    /// Run a remote shell command and return its stdout as a `String`.
    ///
    /// Opens a fresh SSH connection for the call and closes it afterwards.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] on connect, auth, or execution failures,
    /// or if the remote command exits with a non-zero status.
    pub fn run(&self, command: &str) -> Result<String, SdkError> {
        let host = self.host.clone();
        let port = self.port;
        let user = self.user.clone();
        let key_path = self.key_path.clone();
        let command = command.to_owned();
        self.runtime.block_on(async move {
            run_via_openssh(&host, port, &user, &key_path, &command, None).await
        })
    }

    /// Send `stdin_data` to a remote command and return its stdout.
    ///
    /// Used to pipe Content-Length-framed JSON-RPC requests to the SERAPH
    /// binary's stdin on the remote host.
    ///
    /// # Errors
    ///
    /// See [`SshSession::run`].
    pub fn run_with_stdin(&self, command: &str, stdin_data: Vec<u8>) -> Result<String, SdkError> {
        let host = self.host.clone();
        let port = self.port;
        let user = self.user.clone();
        let key_path = self.key_path.clone();
        let command = command.to_owned();
        self.runtime.block_on(async move {
            run_via_openssh(&host, port, &user, &key_path, &command, Some(stdin_data)).await
        })
    }
}

// ── Async transport ─────────────────────────────────────────────────────────

/// Open an SSH connection, run `command` through the remote shell, optionally
/// write `stdin_data` to its stdin, collect stdout, and return it as a `String`.
///
/// Accepts any server host key (`StrictHostKeyChecking=no`) — appropriate for
/// the Khadas ED2P on a dedicated private LAN. Production deployments on public
/// networks should pin a fingerprint.
async fn run_via_openssh(
    host: &str,
    port: u16,
    user: &str,
    key_path: &Path,
    command: &str,
    stdin_data: Option<Vec<u8>>,
) -> Result<String, SdkError> {
    let mut builder = SessionBuilder::default();
    builder.user(user.to_owned());
    builder.port(port);
    builder.keyfile(key_path);
    builder.known_hosts_check(KnownHosts::Accept);

    let session = builder
        .connect(host)
        .await
        .map_err(|e| SdkError::Config(format!("SSH connect {host}:{port}: {e}")))?;

    let output = if let Some(data) = stdin_data {
        let mut child = session
            .shell(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .await
            .map_err(|e| SdkError::Config(format!("SSH spawn: {e}")))?;

        if let Some(stdin) = child.stdin().take() {
            let mut stdin = stdin;
            stdin
                .write_all(&data)
                .await
                .map_err(|e| SdkError::Config(format!("SSH write stdin: {e}")))?;
            // Drop stdin to signal EOF to the remote process.
            drop(stdin);
        }

        child
            .wait_with_output()
            .await
            .map_err(|e| SdkError::Config(format!("SSH wait: {e}")))?
    } else {
        session
            .shell(command)
            .output()
            .await
            .map_err(|e| SdkError::Config(format!("SSH exec: {e}")))?
    };

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        return Err(SdkError::Config(format!(
            "SSH command exited with code {code}"
        )));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| SdkError::Config(format!("stdout is not valid UTF-8: {e}")))
}

/// Resolve the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, unsafe_code)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn ssh_session_new_builds_successfully() {
        let session = SshSession::new("10.129.155.20", 22, "khadas", Path::new("/dev/null"));
        assert!(
            session.is_ok(),
            "SshSession construction failed: {:?}",
            session.err()
        );
    }

    #[test]
    fn khadas_convenience_method_works() {
        let session = SshSession::khadas();
        assert!(session.is_ok(), "khadas() failed: {:?}", session.err());
        let session = session.unwrap();
        assert_eq!(session.host, "10.129.155.20");
        assert_eq!(session.port, 22);
        assert_eq!(session.user, "khadas");
    }

    #[test]
    fn builder_all_required_fields() {
        let session = SshSession::builder()
            .host("10.0.0.1")
            .user("admin")
            .key_path("/dev/null")
            .build();
        assert!(session.is_ok());
        let s = session.unwrap();
        assert_eq!(s.host, "10.0.0.1");
        assert_eq!(s.port, 22);
        assert_eq!(s.user, "admin");
        assert_eq!(s.key_path, PathBuf::from("/dev/null"));
    }

    #[test]
    fn builder_custom_port() {
        let session = SshSession::builder()
            .host("10.0.0.1")
            .port(2222)
            .user("admin")
            .key_path("/dev/null")
            .build()
            .unwrap();
        assert_eq!(session.port, 2222);
    }

    #[test]
    fn builder_missing_host_returns_error() {
        let result = SshSession::builder()
            .user("admin")
            .key_path("/dev/null")
            .build();
        let err = result.expect_err("should fail when host is missing");
        let msg = format!("{err}");
        assert!(msg.contains("host"), "error should mention 'host': {msg}");
    }

    #[test]
    fn builder_missing_user_returns_error() {
        let result = SshSession::builder()
            .host("10.0.0.1")
            .key_path("/dev/null")
            .build();
        let err = result.expect_err("should fail when user is missing");
        let msg = format!("{err}");
        assert!(msg.contains("user"), "error should mention 'user': {msg}");
    }

    #[test]
    fn builder_missing_key_path_returns_error() {
        let result = SshSession::builder().host("10.0.0.1").user("admin").build();
        let err = result.expect_err("should fail when key_path is missing");
        let msg = format!("{err}");
        assert!(
            msg.contains("key_path"),
            "error should mention 'key_path': {msg}"
        );
    }

    #[test]
    fn builder_default_matches_new() {
        let builder = SshSessionBuilder::default();
        assert_eq!(builder.port, 22);
        assert!(builder.host.is_none());
        assert!(builder.user.is_none());
        assert!(builder.key_path.is_none());
    }

    #[test]
    fn env_provider_reads_var() {
        unsafe { std::env::set_var("_SERAPH_SDK_TEST_ENV_PP", "s3cret") };
        let provider = EnvPassphraseProvider::new("_SERAPH_SDK_TEST_ENV_PP");
        let pp = provider.get_passphrase().unwrap();
        assert_eq!(&*pp, "s3cret");
        unsafe { std::env::remove_var("_SERAPH_SDK_TEST_ENV_PP") };
    }

    #[test]
    fn env_provider_missing_var_returns_error() {
        unsafe { std::env::remove_var("_SERAPH_SDK_NONEXISTENT_VAR") };
        let provider = EnvPassphraseProvider::new("_SERAPH_SDK_NONEXISTENT_VAR");
        let result = provider.get_passphrase();
        assert!(result.is_err());
    }

    #[test]
    fn file_provider_reads_first_line() {
        let dir = tempfile::tempdir().unwrap();
        let pp_file = dir.path().join("passphrase.txt");
        {
            let mut f = std::fs::File::create(&pp_file).unwrap();
            writeln!(f, "  my-file-passphrase  ").unwrap();
            writeln!(f, "this-line-is-ignored").unwrap();
        }
        let provider = FilePassphraseProvider::new(&pp_file);
        let pp = provider.get_passphrase().unwrap();
        assert_eq!(&*pp, "my-file-passphrase");
    }

    #[test]
    fn file_provider_missing_file_returns_error() {
        let provider = FilePassphraseProvider::new("/nonexistent/passphrase.txt");
        let result = provider.get_passphrase();
        assert!(result.is_err());
    }

    #[test]
    fn callback_provider_invokes_closure() {
        let provider = CallbackPassphraseProvider::new(Box::new(|| Ok("cb-secret".to_owned())));
        let pp = provider.get_passphrase().unwrap();
        assert_eq!(&*pp, "cb-secret");
    }

    #[test]
    fn callback_provider_propagates_error() {
        let provider = CallbackPassphraseProvider::new(Box::new(|| {
            Err(SdkError::Config("keychain locked".into()))
        }));
        let result = provider.get_passphrase();
        assert!(result.is_err());
    }
}
