//! Container spawn policy types and the `SpawnPolicy` trait.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::container_spawn::disclosure::PHASE_2_DISCLOSURE;

// ── Resource bound constants ──────────────────────────────────────────────────

/// Minimum container memory in MiB.
pub const MIN_MEMORY_MB: u64 = 256;
/// Maximum container memory in MiB (1 TiB — hard upper cap).
pub const MAX_MEMORY_MB_ABSOLUTE: u64 = 1_048_576;
/// Minimum CPU quota as a fractional core count.
pub const MIN_CPUS: f64 = 0.5;
/// Maximum CPU quota as a fractional core count.
pub const MAX_CPUS: f64 = 64.0;
/// Minimum process ID (pids-limit) per container.
pub const MIN_PIDS: u64 = 64;
/// Maximum process ID (pids-limit) per container.
pub const MAX_PIDS: u64 = 8_192;
/// Minimum concurrent containers from a single policy store.
pub const MIN_CONCURRENT: usize = 1;
/// Maximum concurrent containers from a single policy store.
pub const MAX_CONCURRENT: usize = 64;

// ── Isolation mode ────────────────────────────────────────────────────────────

/// Isolation level applied to spawned agent containers.
///
/// Graduated levels layer additional Docker security flags on top of the
/// standard resource limits already applied in `Standard` mode.
///
/// The active level is announced via the `la.iso_mode` Docker label so that
/// AYIN traces can group containers by isolation tier.
///
/// This enum is `#[non_exhaustive]` — downstream crates must include a
/// wildcard arm when matching it.  New isolation tiers may be added in minor
/// releases without a semver bump.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum IsoMode {
    /// Standard resource limits: memory, CPU, pids, `no-new-privileges`.
    #[default]
    Standard,
    /// Hardened: standard + `--cap-drop ALL` + `--cap-add NET_BIND_SERVICE`
    /// + `--user 65534:65534` + read-only root fs + `/tmp` tmpfs (256 MiB).
    ///
    /// Prevents agents from writing to the container filesystem outside of
    /// explicitly mounted tmpfs paths.  `noexec` is intentionally omitted so
    /// agents can execute Python bytecode in `/tmp`.
    Hardened,
    /// Airgapped: Hardened + `--network none`.
    ///
    /// Agents have no outbound network access.  Use when the task is purely
    /// local (code analysis, refactoring) and must not exfiltrate data.
    ///
    /// **Requires** `NetworkPolicy::None`; [`ContainerPolicy::validate`]
    /// returns an error if any other network policy is combined with this
    /// iso mode.
    Airgapped,
}

impl IsoMode {
    /// Returns the value written to the `la.iso_mode` Docker label.
    #[must_use]
    pub fn as_label_value(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Hardened => "hardened",
            Self::Airgapped => "airgapped",
        }
    }

    /// Returns `true` if this level requires a read-only root filesystem.
    #[must_use]
    pub fn requires_read_only_root(self) -> bool {
        matches!(self, Self::Hardened | Self::Airgapped)
    }
}

// ── Network policy ────────────────────────────────────────────────────────────

/// Network connectivity granted to agent containers.
///
/// This enum is `#[non_exhaustive]` — downstream crates must include a
/// wildcard arm when matching.  `Balanced` is declared for Phase-2 API
/// stability but returns [`SpawnError::NotYetImplemented`] from
/// [`ContainerPolicy::build_docker_args`] until Phase 2 ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum NetworkPolicy {
    /// Standard Docker bridge network — agents can reach the internet via
    /// NAT.  Default for most interactive builds.
    #[default]
    Bridge,
    /// Share the host network stack — no network isolation.  Use only for
    /// debugging or when the agent must bind privileged ports.
    Host,
    /// No network interface — `--network none`.  Required for
    /// [`IsoMode::Airgapped`] and recommended for pure-analysis tasks.
    None,
    /// **Phase 2 — not yet implemented.**
    ///
    /// Split-horizon: agent-to-agent traffic bridged; outbound internet
    /// blocked.  Declared now for API stability.  Calling
    /// [`ContainerPolicy::build_docker_args`] with this variant returns
    /// [`SpawnError::NotYetImplemented`].
    Balanced,
    /// Dedicated `la-worker-bridge` Docker bridge network.
    ///
    /// Worker-task containers run on this isolated network rather than the
    /// default `docker0` bridge, providing namespace separation from PTY
    /// session containers (SERAPH T8 mitigation).
    WorkerBridge,
}

// ── Credential strategy ───────────────────────────────────────────────────────

/// How credentials are supplied to agent containers.
///
/// This enum is `#[non_exhaustive]` — downstream crates must include a
/// wildcard arm when matching.  `Proxy` is declared for Phase-2 API stability
/// but is not yet implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum CredentialStrategy {
    /// Inherit the caller's filtered environment (PATH, HOME, LA_* vars only —
    /// no raw API keys forwarded into the container).
    #[default]
    Inherit,
    /// Empty environment — no credentials forwarded.
    None,
    /// **Phase 2 — not yet implemented.**
    ///
    /// Credentials vended on-demand via the gateway credential-proxy route.
    /// Declared now so call-sites can be updated without a breaking change.
    Proxy,
}

// ── Agent tier ────────────────────────────────────────────────────────────────

/// Predefined resource tiers for agent containers.
///
/// Tiers map to opinionated `ContainerResources` defaults.  Use `Custom`
/// when the predefined limits do not fit the workload, then populate
/// `ContainerPolicy::resources` explicitly.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentTier {
    /// 512 MiB memory · 0.5 CPU · 64 pids · max 2 concurrent.
    Micro,
    /// 2 GiB memory · 2 CPU · 256 pids · max 4 concurrent.  Default.
    #[default]
    Standard,
    /// 8 GiB memory · 4 CPU · 512 pids · max 8 concurrent.
    Large,
    /// User-supplied resources via `ContainerPolicy::resources`.
    Custom,
}

impl AgentTier {
    /// Returns the default resource limits for this tier.
    #[must_use]
    pub fn default_resources(self) -> ContainerResources {
        match self {
            Self::Micro => ContainerResources {
                memory_mb: 512,
                cpus: 0.5,
                pids_limit: 64,
                max_concurrent: 2,
            },
            Self::Standard => ContainerResources {
                memory_mb: 2_048,
                cpus: 2.0,
                pids_limit: 256,
                max_concurrent: 4,
            },
            Self::Large => ContainerResources {
                memory_mb: 8_192,
                cpus: 4.0,
                pids_limit: 512,
                max_concurrent: 8,
            },
            Self::Custom => ContainerResources {
                memory_mb: MIN_MEMORY_MB,
                cpus: MIN_CPUS,
                pids_limit: MIN_PIDS,
                max_concurrent: MIN_CONCURRENT,
            },
        }
    }
}

// ── Container resources ───────────────────────────────────────────────────────

/// Resource caps for a container.
///
/// All fields are validated against their `MIN_*` / `MAX_*` constants by
/// [`ContainerPolicy::validate`].
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerResources {
    /// Memory limit in MiB.  Range: `[MIN_MEMORY_MB, MAX_MEMORY_MB_ABSOLUTE]`.
    pub memory_mb: u64,
    /// CPU quota as a fractional core count.  Range: `[MIN_CPUS, MAX_CPUS]`.
    pub cpus: f64,
    /// Maximum number of processes/threads in the container.  Range:
    /// `[MIN_PIDS, MAX_PIDS]`.
    pub pids_limit: u64,
    /// Maximum number of containers that may run concurrently from this
    /// policy store.  Range: `[MIN_CONCURRENT, MAX_CONCURRENT]`.
    pub max_concurrent: usize,
}

impl ContainerResources {
    /// Returns `true` if `self` is at least as restrictive as `other` on
    /// every resource dimension.
    ///
    /// Used by [`PolicyStore::tighten_for_build`] to enforce the
    /// monotonic-tightening invariant (SERAPH constraint).
    #[must_use]
    pub fn is_at_least_as_tight_as(&self, other: &Self) -> bool {
        self.memory_mb <= other.memory_mb
            && self.cpus <= other.cpus
            && self.pids_limit <= other.pids_limit
            && self.max_concurrent <= other.max_concurrent
    }
}

// ── Container policy ──────────────────────────────────────────────────────────

/// Full container spawn policy combining isolation, networking, credentials,
/// and resource caps.
///
/// Build a policy with `ContainerPolicy::default()` (Standard tier, Bridge
/// network, no seccomp profile) and adjust fields as needed before calling
/// [`ContainerPolicy::validate`].
#[derive(Debug, Clone)]
pub struct ContainerPolicy {
    /// Isolation level — controls seccomp, cap-drop, userns, and fs flags.
    pub iso_mode: IsoMode,
    /// Network connectivity granted to the container.
    pub network: NetworkPolicy,
    /// How credentials are supplied to the container process.
    pub credentials: CredentialStrategy,
    /// Predefined resource tier.  Ignored when `tier == AgentTier::Custom`
    /// and `resources` is set explicitly.
    pub tier: AgentTier,
    /// Resource caps.  When `tier != AgentTier::Custom` these are
    /// auto-populated from `tier.default_resources()`.
    pub resources: ContainerResources,
    /// Path to the seccomp profile JSON written to disk before spawning.
    ///
    /// When `None`, `--security-opt no-new-privileges` is still passed but
    /// no custom seccomp profile is applied.  Set this to the path produced
    /// by writing [`crate::container_spawn::seccomp::SECCOMP_PROFILE_JSON`]
    /// to a temp file for Hardened/Airgapped containers.
    pub seccomp_profile_path: Option<std::path::PathBuf>,
}

impl Default for ContainerPolicy {
    fn default() -> Self {
        let tier = AgentTier::Standard;
        Self {
            iso_mode: IsoMode::default(),
            network: NetworkPolicy::default(),
            credentials: CredentialStrategy::default(),
            resources: tier.default_resources(),
            tier,
            seccomp_profile_path: Option::default(),
        }
    }
}

impl ContainerPolicy {
    /// Validates all fields against their declared invariants.
    ///
    /// Returns `Err` if:
    /// - `IsoMode::Airgapped` is combined with any network policy other than
    ///   `NetworkPolicy::None`
    /// - Any resource dimension is outside its `[MIN_*, MAX_*]` range
    /// - `CredentialStrategy::Proxy` is requested (Phase 2 only)
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError`] describing the first constraint violated.
    pub fn validate(&self) -> Result<(), SpawnError> {
        if self.iso_mode == IsoMode::Airgapped && self.network != NetworkPolicy::None {
            return Err(SpawnError::PolicyConflict(
                "IsoMode::Airgapped requires NetworkPolicy::None".to_owned(),
            ));
        }
        if matches!(self.credentials, CredentialStrategy::Proxy) {
            return Err(SpawnError::NotYetImplemented(PHASE_2_DISCLOSURE));
        }
        if self.resources.memory_mb < MIN_MEMORY_MB {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "memory_mb {} is below MIN_MEMORY_MB {}",
                self.resources.memory_mb, MIN_MEMORY_MB
            )));
        }
        if self.resources.memory_mb > MAX_MEMORY_MB_ABSOLUTE {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "memory_mb {} exceeds MAX_MEMORY_MB_ABSOLUTE {}",
                self.resources.memory_mb, MAX_MEMORY_MB_ABSOLUTE
            )));
        }
        if !self.resources.cpus.is_finite() {
            return Err(SpawnError::ResourceOutOfBounds(
                "cpus must be finite (NaN and ±Infinity are not valid)".to_owned(),
            ));
        }
        if self.resources.cpus < MIN_CPUS {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "cpus {:.2} is below MIN_CPUS {:.2}",
                self.resources.cpus, MIN_CPUS
            )));
        }
        if self.resources.cpus > MAX_CPUS {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "cpus {:.2} exceeds MAX_CPUS {:.2}",
                self.resources.cpus, MAX_CPUS
            )));
        }
        if self.resources.pids_limit < MIN_PIDS {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "pids_limit {} is below MIN_PIDS {}",
                self.resources.pids_limit, MIN_PIDS
            )));
        }
        if self.resources.pids_limit > MAX_PIDS {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "pids_limit {} exceeds MAX_PIDS {}",
                self.resources.pids_limit, MAX_PIDS
            )));
        }
        if self.resources.max_concurrent < MIN_CONCURRENT {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "max_concurrent {} is below MIN_CONCURRENT {}",
                self.resources.max_concurrent, MIN_CONCURRENT
            )));
        }
        if self.resources.max_concurrent > MAX_CONCURRENT {
            return Err(SpawnError::ResourceOutOfBounds(format!(
                "max_concurrent {} exceeds MAX_CONCURRENT {}",
                self.resources.max_concurrent, MAX_CONCURRENT
            )));
        }
        Ok(())
    }

    /// Builds the `docker run` argument list in the canonical pinned order.
    ///
    /// **Pinned order** (matches the plan's SERAPH-audited sequence):
    /// `--security-opt no-new-privileges` →
    /// `--security-opt seccomp=<path>` →
    /// `--cap-drop ALL` → `--cap-add NET_BIND_SERVICE` →
    /// `--user 65534:65534` →
    /// `--memory <N>m` → `--cpus <N>` → `--pids-limit <N>` →
    /// `--network <mode>` →
    /// `--read-only` → `--tmpfs /tmp:rw,nosuid,size=256m` →
    /// `--label la.iso_mode=<level>` →
    /// `--restart no`
    ///
    /// The image name is NOT included — the caller appends it after this
    /// slice.
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError::NotYetImplemented`] if `network` is
    /// [`NetworkPolicy::Balanced`] (Phase 2).
    pub fn build_docker_args(&self) -> Result<Vec<String>, SpawnError> {
        let mut args: Vec<String> = Vec::with_capacity(24);

        // 1. --security-opt no-new-privileges (always)
        args.push("--security-opt".to_owned());
        args.push("no-new-privileges".to_owned());

        // 2. --security-opt seccomp=<path>
        if let Some(path) = &self.seccomp_profile_path {
            args.push("--security-opt".to_owned());
            args.push(format!("seccomp={}", path.display()));
        }

        // 3–5. cap-drop/cap-add/user (Hardened/Airgapped)
        if self.iso_mode.requires_read_only_root() {
            append_hardened_caps_and_user(&mut args);
        }

        // 6. --memory
        args.push("--memory".to_owned());
        args.push(format!("{}m", self.resources.memory_mb));

        // 7. --cpus
        args.push("--cpus".to_owned());
        args.push(format!("{:.2}", self.resources.cpus));

        // 8. --pids-limit
        args.push("--pids-limit".to_owned());
        args.push(self.resources.pids_limit.to_string());

        // 9. --network
        // When new NetworkPolicy variants are added, update this match or
        // add them to the NotYetImplemented arm before they reach production.
        let network_flag = match self.network {
            NetworkPolicy::Bridge => "bridge",
            NetworkPolicy::Host => "host",
            NetworkPolicy::None => "none",
            NetworkPolicy::WorkerBridge => "la-worker-bridge",
            NetworkPolicy::Balanced => {
                return Err(SpawnError::NotYetImplemented(PHASE_2_DISCLOSURE));
            }
        };
        args.push("--network".to_owned());
        args.push(network_flag.to_owned());

        // 10–11. --read-only + --tmpfs /tmp:rw,nosuid,size=256m (Hardened/Airgapped)
        // NOTE: `noexec` intentionally absent — agents execute Python bytecode in /tmp.
        if self.iso_mode.requires_read_only_root() {
            append_hardened_fs(&mut args);
        }

        // 12. --label la.iso_mode=<level>
        args.push("--label".to_owned());
        args.push(format!("la.iso_mode={}", self.iso_mode.as_label_value()));

        // 13. --restart no (agent containers never restart automatically)
        args.push("--restart".to_owned());
        args.push("no".to_owned());

        Ok(args)
    }
}

// ── SpawnPolicy trait ─────────────────────────────────────────────────────────

/// Trait for types that hold and serve the active container spawn policy.
///
/// The canonical implementation is [`PolicyStore`], which uses
/// `Arc<ArcSwap<ContainerPolicy>>` for ~10 ns lock-free reads.
pub trait SpawnPolicy: Send + Sync {
    /// Returns the current system-wide effective policy.
    ///
    /// Callers receive a cheap `Arc` clone with no locking.
    fn effective_policy(&self) -> Arc<ContainerPolicy>;

    /// Derives a per-build policy by applying a tightening override.
    ///
    /// Only *tighter* (more restrictive) values are accepted in every
    /// resource dimension.  The override does not mutate the system-wide
    /// policy; it returns a new `Arc<ContainerPolicy>` for the specific
    /// build.
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError::PolicyTighteningViolation`] if the override
    /// would *loosen* any resource limit compared to `effective_policy()`.
    fn tighten_for_build(
        &self,
        override_policy: &ContainerPolicy,
    ) -> Result<Arc<ContainerPolicy>, SpawnError>;

    /// Replaces the system-wide policy (admin operation).
    ///
    /// Unlike `tighten_for_build`, this is a privileged mutation — there is
    /// no monotonic constraint.  Access must be gated behind operator
    /// authorization at the call site.
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError`] if `new_policy` fails [`ContainerPolicy::validate`].
    fn update_system_policy(&self, new_policy: ContainerPolicy) -> Result<(), SpawnError>;
}

// ── PolicyStore ───────────────────────────────────────────────────────────────

/// Lock-free container policy store.
///
/// Wraps `Arc<ArcSwap<ContainerPolicy>>` for ~10 ns reads on the hot path.
/// Use the single-load idiom to avoid holding a reference across an `await`
/// point:
///
/// ```rust
/// # use std::sync::Arc;
/// # use lightarchitects::container_spawn::policy::{
/// #     ContainerPolicy, PolicyStore, SpawnPolicy,
/// # };
/// let store = PolicyStore::new(ContainerPolicy::default())?;
/// // Load once — cheap Arc clone, no lock.
/// let policy: Arc<ContainerPolicy> = store.effective_policy();
/// // Use `policy` synchronously before the next await point.
/// drop(policy);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// If you need to read the policy across an `await` boundary, call
/// `effective_policy()` again after the yield point rather than holding the
/// `Arc` through it.
pub struct PolicyStore {
    inner: Arc<ArcSwap<ContainerPolicy>>,
}

impl PolicyStore {
    /// Creates a new `PolicyStore` with the supplied initial policy.
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError`] if `initial` fails [`ContainerPolicy::validate`].
    pub fn new(initial: ContainerPolicy) -> Result<Self, SpawnError> {
        initial.validate()?;
        Ok(Self {
            inner: Arc::new(ArcSwap::from_pointee(initial)),
        })
    }

    /// Returns a shared handle to this store.
    ///
    /// Cloning the handle is `O(1)` — it increments one reference count on
    /// the outer `Arc`.  All clones share the same live policy.
    #[must_use]
    pub fn handle(&self) -> Arc<ArcSwap<ContainerPolicy>> {
        Arc::clone(&self.inner)
    }
}

impl SpawnPolicy for PolicyStore {
    fn effective_policy(&self) -> Arc<ContainerPolicy> {
        self.inner.load_full()
    }

    fn tighten_for_build(
        &self,
        override_policy: &ContainerPolicy,
    ) -> Result<Arc<ContainerPolicy>, SpawnError> {
        override_policy.validate()?;
        let current = self.inner.load_full();
        if !override_policy
            .resources
            .is_at_least_as_tight_as(&current.resources)
        {
            return Err(SpawnError::PolicyTighteningViolation(format!(
                "per-build override must not loosen limits: \
                 current=(mem={mem}MiB, cpus={cpus:.2}, pids={pids}, concurrent={con}), \
                 override=(mem={omem}MiB, cpus={ocpus:.2}, pids={opids}, concurrent={ocon})",
                mem = current.resources.memory_mb,
                cpus = current.resources.cpus,
                pids = current.resources.pids_limit,
                con = current.resources.max_concurrent,
                omem = override_policy.resources.memory_mb,
                ocpus = override_policy.resources.cpus,
                opids = override_policy.resources.pids_limit,
                ocon = override_policy.resources.max_concurrent,
            )));
        }
        // Isolation level must be at least as strict.
        if isolation_rank(override_policy.iso_mode) < isolation_rank(current.iso_mode) {
            return Err(SpawnError::PolicyTighteningViolation(format!(
                "per-build IsoMode {:?} is less strict than system IsoMode {:?}",
                override_policy.iso_mode, current.iso_mode
            )));
        }
        // Network policy must be at least as strict.
        if network_rank(override_policy.network) < network_rank(current.network) {
            return Err(SpawnError::PolicyTighteningViolation(format!(
                "per-build NetworkPolicy {:?} is less strict than system NetworkPolicy {:?}",
                override_policy.network, current.network
            )));
        }
        Ok(Arc::new(override_policy.clone()))
    }

    fn update_system_policy(&self, new_policy: ContainerPolicy) -> Result<(), SpawnError> {
        new_policy.validate()?;
        self.inner.store(Arc::new(new_policy));
        Ok(())
    }
}

impl Default for PolicyStore {
    /// Creates a `PolicyStore` from [`ContainerPolicy::default()`].
    ///
    /// The default policy always passes validation; this constructor is
    /// infallible. Use [`PolicyStore::new`] when you need to supply a
    /// custom policy and handle the validation error.
    fn default() -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(ContainerPolicy::default())),
        }
    }
}

/// Returns a monotonically increasing rank for isolation levels so that
/// `tighten_for_build` can enforce that overrides are never less strict.
fn isolation_rank(mode: IsoMode) -> u8 {
    match mode {
        IsoMode::Standard => 0,
        IsoMode::Hardened => 1,
        IsoMode::Airgapped => 2,
    }
}

/// Returns a monotonically increasing rank for network policies so that
/// `tighten_for_build` can enforce that overrides are never less strict.
fn network_rank(policy: NetworkPolicy) -> u8 {
    match policy {
        NetworkPolicy::Host => 0,
        NetworkPolicy::Bridge | NetworkPolicy::Balanced | NetworkPolicy::WorkerBridge => 1,
        NetworkPolicy::None => 2,
    }
}

/// Appends `--cap-drop ALL`, `--cap-add NET_BIND_SERVICE`, and
/// `--user 65534:65534` for Hardened/Airgapped containers.
fn append_hardened_caps_and_user(args: &mut Vec<String>) {
    args.push("--cap-drop".to_owned());
    args.push("ALL".to_owned());
    args.push("--cap-add".to_owned());
    args.push("NET_BIND_SERVICE".to_owned());
    args.push("--user".to_owned());
    args.push("65534:65534".to_owned());
}

/// Appends `--read-only` and `--tmpfs /tmp:rw,nosuid,size=256m` for
/// Hardened/Airgapped containers.  `noexec` is intentionally absent.
fn append_hardened_fs(args: &mut Vec<String>) {
    args.push("--read-only".to_owned());
    args.push("--tmpfs".to_owned());
    args.push("/tmp:rw,nosuid,size=256m".to_owned());
}

// ── SpawnError ────────────────────────────────────────────────────────────────

/// Errors produced by container spawn policy operations.
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    /// A Phase-2 feature was requested before it was implemented.
    #[error("not yet implemented: {0}")]
    NotYetImplemented(&'static str),

    /// A per-build policy override would loosen a resource limit.
    #[error("policy tightening violation: {0}")]
    PolicyTighteningViolation(String),

    /// A resource dimension is outside its declared `[MIN_*, MAX_*]` range.
    #[error("resource out of bounds: {0}")]
    ResourceOutOfBounds(String),

    /// Two policy fields conflict with each other (e.g. Airgapped + non-None network).
    #[error("policy conflict: {0}")]
    PolicyConflict(String),
}
