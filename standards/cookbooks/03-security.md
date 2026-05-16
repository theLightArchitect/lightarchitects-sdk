<!-- uuid: cfaed4d6-0ba3-429f-9fac-3d2139614a41 -->

---
id: "e5a8fa15-bd891eef"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# Cookbook 03: Security Patterns

**Purpose**: Security patterns for Rust MCP servers
**Key Concept**: Defense in depth - validate inputs, sanitize outputs, protect secrets
**Audience**: All MCP server developers

---

## Security Principles

1. **Never Trust Input**: All external data is potentially malicious
2. **Least Privilege**: Minimal permissions required
3. **Defense in Depth**: Multiple security layers
4. **Fail Securely**: Default to deny, not allow
5. **Audit Everything**: Log security-relevant events

---

## Input Sanitization

### String Sanitization

```rust
// src/security/sanitizers.rs

/// Remove control characters, limit length
pub fn sanitize_string(input: &str, max_length: usize) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(max_length)
        .collect()
}

/// Sanitize for safe file path component
pub fn sanitize_path_component(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .take(255)
        .collect()
}

/// Escape for safe shell argument (prefer avoiding shell entirely)
pub fn escape_shell_arg(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}
```

### Path Traversal Prevention

```rust
use std::path::{Path, PathBuf};

/// Safely join user input to base directory, preventing traversal
pub fn safe_path_join(base: &Path, user_input: &str) -> Result<PathBuf, SecurityError> {
    // Normalize and remove any path traversal
    let sanitized = user_input
        .replace("..", "")
        .replace("//", "/")
        .trim_start_matches('/')
        .to_string();

    let joined = base.join(&sanitized);

    // Verify result is still under base
    let canonical = joined.canonicalize()
        .map_err(|_| SecurityError::InvalidPath)?;
    let base_canonical = base.canonicalize()
        .map_err(|_| SecurityError::InvalidPath)?;

    if !canonical.starts_with(&base_canonical) {
        return Err(SecurityError::PathTraversal);
    }

    Ok(canonical)
}

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Path traversal attempt detected")]
    PathTraversal,
    #[error("Invalid path")]
    InvalidPath,
}
```

---

## Injection Prevention

### Query Injection Guard

```rust
/// Patterns that indicate injection attempts
const INJECTION_PATTERNS: &[&str] = &[
    "'; --",
    "\" OR ",
    "' OR ",
    "1=1",
    "DROP TABLE",
    "DELETE FROM",
    "<script>",
    "javascript:",
];

pub fn detect_injection(input: &str) -> bool {
    let upper = input.to_uppercase();
    INJECTION_PATTERNS.iter().any(|pattern| upper.contains(&pattern.to_uppercase()))
}

pub fn sanitize_query_input(input: &str) -> Result<String, SecurityError> {
    if detect_injection(input) {
        return Err(SecurityError::InjectionAttempt);
    }

    Ok(sanitize_string(input, 1000))
}
```

### Command Execution Safety

```rust
use std::process::Command;

/// Safe command execution (avoid shell entirely)
pub fn execute_command_safe(
    program: &str,
    args: &[&str],
    timeout_secs: u64,
) -> Result<String, Error> {
    // Allowlist of permitted programs
    const ALLOWED_PROGRAMS: &[&str] = &["grep", "jq", "head", "tail"];

    if !ALLOWED_PROGRAMS.contains(&program) {
        return Err(Error::DisallowedProgram(program.to_string()));
    }

    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| Error::CommandFailed(e.to_string()))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(Error::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ))
    }
}
```

---

## Secrets Management

### Never Hardcode Secrets

```rust
// WRONG
const API_KEY: &str = "sk-ant-api03-...";  // NEVER DO THIS

// RIGHT
fn get_api_key() -> Result<String, Error> {
    std::env::var("API_KEY")
        .map_err(|_| Error::MissingConfig("API_KEY environment variable not set".to_string()))
}
```

### Secure Configuration Loading

```rust
use std::collections::HashMap;

pub struct SecureConfig {
    values: HashMap<String, String>,
}

impl SecureConfig {
    pub fn from_env(required_keys: &[&str]) -> Result<Self, Error> {
        let mut values = HashMap::new();
        let mut missing = Vec::new();

        for key in required_keys {
            match std::env::var(key) {
                Ok(value) => { values.insert(key.to_string(), value); }
                Err(_) => { missing.push(*key); }
            }
        }

        if !missing.is_empty() {
            return Err(Error::MissingConfig(
                format!("Missing required environment variables: {:?}", missing)
            ));
        }

        Ok(Self { values })
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }
}
```

### Secrets in Logs

```rust
/// Redact sensitive patterns from strings before logging
pub fn redact_secrets(input: &str) -> String {
    let patterns = [
        (r"sk-ant-[a-zA-Z0-9-_]{40,}", "[REDACTED_ANTHROPIC_KEY]"),
        (r"AIza[0-9A-Za-z-_]{35}", "[REDACTED_GOOGLE_KEY]"),
        (r"AKIA[0-9A-Z]{16}", "[REDACTED_AWS_KEY]"),
        (r"Bearer [A-Za-z0-9._-]+", "Bearer [REDACTED]"),
        (r"password[\"']?\s*[:=]\s*[\"']?[^\"'\s]+", "password=[REDACTED]"),
    ];

    let mut result = input.to_string();
    for (pattern, replacement) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, replacement).to_string();
        }
    }
    result
}
```

---

## PII Protection (DataGuard)

```rust
/// PII detection and redaction
pub struct DataGuard {
    email_pattern: regex::Regex,
    phone_pattern: regex::Regex,
    ip_pattern: regex::Regex,
}

impl DataGuard {
    pub fn new() -> Self {
        Self {
            email_pattern: regex::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
            phone_pattern: regex::Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap(),
            ip_pattern: regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
        }
    }

    pub fn redact_pii(&self, input: &str) -> String {
        let mut result = input.to_string();
        result = self.email_pattern.replace_all(&result, "[EMAIL_REDACTED]").to_string();
        result = self.phone_pattern.replace_all(&result, "[PHONE_REDACTED]").to_string();
        result = self.ip_pattern.replace_all(&result, "[IP_REDACTED]").to_string();
        result
    }

    pub fn contains_pii(&self, input: &str) -> bool {
        self.email_pattern.is_match(input) ||
        self.phone_pattern.is_match(input) ||
        self.ip_pattern.is_match(input)
    }
}
```

---

## Audit Logging

```rust
use chrono::Utc;
use serde::Serialize;

#[derive(Serialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub event_type: String,
    pub tool_name: String,
    pub user_hash: String,  // Never log actual user IDs
    pub params_hash: String, // Hash, not actual params
    pub result_status: String,
    pub execution_time_ms: u64,
}

impl AuditEvent {
    pub fn new(tool_name: &str, user_id: &str, params: &serde_json::Value) -> Self {
        use sha2::{Sha256, Digest};

        let mut hasher = Sha256::new();
        hasher.update(user_id.as_bytes());
        let user_hash = format!("{:x}", hasher.finalize())[..16].to_string();

        let mut hasher = Sha256::new();
        hasher.update(params.to_string().as_bytes());
        let params_hash = format!("{:x}", hasher.finalize())[..16].to_string();

        Self {
            timestamp: Utc::now().to_rfc3339(),
            event_type: "tool_call".to_string(),
            tool_name: tool_name.to_string(),
            user_hash,
            params_hash,
            result_status: "pending".to_string(),
            execution_time_ms: 0,
        }
    }
}
```

---

## Security Hook Example

```rust
// src/hooks/builtin/security_validation.rs

use crate::hooks::{PreToolUse, HookContext, HookResult};
use crate::security::{DataGuard, detect_injection};
use async_trait::async_trait;

pub struct SecurityValidationHook {
    data_guard: DataGuard,
}

impl SecurityValidationHook {
    pub fn new() -> Self {
        Self { data_guard: DataGuard::new() }
    }
}

#[async_trait]
impl PreToolUse for SecurityValidationHook {
    fn name(&self) -> &'static str { "security_validation" }
    fn priority(&self) -> i32 { 3 } // Run before input validation

    async fn execute(&self, ctx: HookContext) -> HookResult {
        let params_str = ctx.params.to_string();

        // Check for injection attempts
        if detect_injection(&params_str) {
            tracing::warn!(
                tool = ctx.tool_name,
                "Potential injection attempt detected"
            );
            return HookResult::Block {
                reason: "Security validation failed".to_string(),
                remediation: Some("Remove potentially malicious content from input".to_string()),
            };
        }

        // Check for PII in inputs (warn, don't block)
        if self.data_guard.contains_pii(&params_str) {
            tracing::warn!(
                tool = ctx.tool_name,
                "PII detected in tool input"
            );
            // Could require review instead of blocking
        }

        HookResult::Continue(ctx)
    }
}
```

---

## Secure Error Messages

```rust
/// Sanitize error messages before returning to client
pub fn sanitize_error_message(error: &str) -> String {
    // Remove file paths
    let result = regex::Regex::new(r"/[\w/.-]+")
        .unwrap()
        .replace_all(error, "[PATH]")
        .to_string();

    // Remove potential secrets
    redact_secrets(&result)
}

// In error handling
impl From<InternalError> for PublicError {
    fn from(e: InternalError) -> Self {
        // Log full error internally
        tracing::error!("Internal error: {:?}", e);

        // Return sanitized version to client
        PublicError {
            message: sanitize_error_message(&e.to_string()),
            code: e.code(),
        }
    }
}
```

---

## Checklist

Before deploying any MCP server:

- [ ] All user input is validated and sanitized
- [ ] No hardcoded secrets in code
- [ ] Secrets loaded from environment variables
- [ ] File paths validated against traversal
- [ ] Command execution uses allowlist (if any)
- [ ] PII detection/redaction in place
- [ ] Error messages don't leak internal details
- [ ] Audit logging for security events
- [ ] Rate limiting enabled
- [ ] HTTPS/TLS for any network communication

---

## Next Steps

- **[04-provider.md](./04-provider.md)** - AI provider security (tier routing)
- **[01-foundations.md](./01-foundations.md)** - Security hooks integration

---

*Security patterns for any Rust MCP server*

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
- [[user/standards/canon/builders-cookbook|Builders Cookbook]]
