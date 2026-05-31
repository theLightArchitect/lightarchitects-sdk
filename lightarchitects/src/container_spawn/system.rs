//! Host-system detection for `ContainerResources::from_system()`.

use crate::container_spawn::policy::{
    ContainerResources, MAX_CONCURRENT, MAX_CPUS, MAX_MEMORY_MB_ABSOLUTE, MIN_CONCURRENT, MIN_CPUS,
    MIN_MEMORY_MB, MIN_PIDS,
};

impl ContainerResources {
    /// Probes the host to determine sensible default resource caps.
    ///
    /// Caps are set to half the detected host capacity so the host OS
    /// and other workloads retain headroom:
    ///
    /// - `memory_mb` = `host_ram / 2`, clamped to `[MIN_MEMORY_MB * 4, MAX_MEMORY_MB_ABSOLUTE / 2]`
    /// - `cpus` = `host_logical_cpus / 2`, clamped to `[MIN_CPUS * 2, MAX_CPUS / 2]`
    /// - `pids_limit` = 256 (conservative default; tuned by operators)
    /// - `max_concurrent` = `4` (tuned by operators)
    ///
    /// On failure to detect any dimension, conservative defaults are used
    /// (`MIN_MEMORY_MB * 4` / `MIN_CPUS * 2`) rather than erroring, so
    /// spawn always succeeds even in restricted probe environments (LXC, VM,
    /// exotic kernel configurations).
    #[must_use]
    pub fn from_system() -> Self {
        let raw_memory_mb = detect_memory_mb().unwrap_or(MIN_MEMORY_MB * 8);
        let raw_cpus = detect_cpu_count().unwrap_or(2.0);

        let memory_mb = (raw_memory_mb / 2).clamp(MIN_MEMORY_MB * 4, MAX_MEMORY_MB_ABSOLUTE / 2);

        let cpus = (raw_cpus / 2.0).clamp(MIN_CPUS * 2.0, MAX_CPUS / 2.0);

        Self {
            memory_mb,
            cpus,
            pids_limit: MIN_PIDS * 4,
            max_concurrent: (MAX_CONCURRENT / 16).max(MIN_CONCURRENT),
        }
    }
}

// ── macOS detection ───────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn detect_memory_mb() -> Option<u64> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = std::str::from_utf8(&out.stdout).ok()?;
    let bytes = s.trim().parse::<u64>().ok()?;
    Some(bytes / 1_048_576)
}

#[cfg(target_os = "macos")]
fn detect_cpu_count() -> Option<f64> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", "hw.logicalcpu"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = std::str::from_utf8(&out.stdout).ok()?;
    s.trim().parse::<f64>().ok()
}

// ── Linux detection ───────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn detect_memory_mb() -> Option<u64> {
    // Try cgroup-v2 first: limit may be lower than physical RAM.
    if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/memory.max") {
        let trimmed = s.trim();
        if trimmed != "max" {
            if let Ok(bytes) = trimmed.parse::<u64>() {
                return Some(bytes / 1_048_576);
            }
        }
    }
    // Fall back to /proc/meminfo MemTotal.
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                let mut parts = line.split_ascii_whitespace();
                let _label = parts.next();
                if let Some(kb_str) = parts.next() {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return Some(kb / 1_024);
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_cpu_count() -> Option<f64> {
    // Try cgroup-v2 CPU quota: "200000 100000" → 2.0 CPUs.
    if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/cpu.max") {
        let trimmed = s.trim();
        if trimmed != "max" {
            let mut parts = trimmed.split_ascii_whitespace();
            if let (Some(quota_s), Some(period_s)) = (parts.next(), parts.next()) {
                if let (Ok(quota), Ok(period)) = (quota_s.parse::<f64>(), period_s.parse::<f64>()) {
                    if period > 0.0 {
                        return Some(quota / period);
                    }
                }
            }
        }
    }
    // Fall back to logical CPU count from /proc/cpuinfo.
    if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
        let count = content
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count();
        if count > 0 {
            return Some(count as f64);
        }
    }
    None
}

// ── Windows detection ─────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn detect_memory_mb() -> Option<u64> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let bytes = sys.total_memory();
    if bytes == 0 {
        None
    } else {
        Some(bytes / 1_048_576)
    }
}

#[cfg(target_os = "windows")]
fn detect_cpu_count() -> Option<f64> {
    let count = sysinfo::System::physical_core_count()?;
    Some(count as f64)
}

// ── Fallback for other platforms ─────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect_memory_mb() -> Option<u64> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect_cpu_count() -> Option<f64> {
    None
}
