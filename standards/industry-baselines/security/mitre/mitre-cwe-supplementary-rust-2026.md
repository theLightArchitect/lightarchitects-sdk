<!-- uuid: b4e2c9d1-7f3a-4812-b9e0-1a5c8f2d6e04 -->
<!-- source: authored | version: 2026-05-22 | gate: [S] -->
<!-- purpose: Rust-specific mitigations for CWE weaknesses not in CWE Top 25 2025 but validated in LA platform security audits -->

# CWE Supplementary — Rust Security Patterns (Light Architects Platform)

> Companion to `mitre-cwe-top-25-2025-*.md`. These weaknesses are outside the Top 25 but were
> directly validated in platform security engagements. Rust-specific mitigations are canonical.

---

## CWE-208 — Observable Timing Discrepancy

**Description**: Two separate execution paths take different amounts of time to execute due to
differences in the operands. An attacker can observe the time difference to infer secret values.

**Platform context**: Bearer token prefix checks using variable-length slicing before `ct_eq()`
create a timing oracle for sub-prefix-length tokens. Discovered: ENG-KHADAS-AUDIT-20260522 (F-2).

**Rust mitigation** (validated, production-deployed):
```rust
// WRONG — Option::is_some_and short-circuits for tokens shorter than N bytes
token.as_bytes().get(..4).is_some_and(|b| b.ct_eq(b"lak_").into())

// CORRECT — unconditional pad to N, then ct_eq (no short-circuit possible)
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
let token_bytes = token.as_bytes();
let mut padded = Zeroizing::new([0u8; 4]);
let copy_len = token_bytes.len().min(4);
padded[..copy_len].copy_from_slice(&token_bytes[..copy_len]);
let matches: bool = padded.ct_eq(b"prefix").into();
```

**Key invariants**:
- Pad length must be a compile-time constant (avoids new timing leak on `min()` result)
- Buffer must be `Zeroizing<[u8; N]>` to also close CWE-316 on the rejection path
- `subtle::ConstantTimeEq` is the only approved crate for CT comparison on this platform
- Any use of `==`, `.eq()`, or `PartialEq` on secret bytes is a gate [S] MEDIUM finding

**Severity** (platform classification): MEDIUM  
**Confidence threshold**: HIGH (≥95%) — CWE-208 is empirically observable via timing side-channel

---

## CWE-316 — Cleartext Storage of Sensitive Information in Memory

**Description**: A program stores sensitive information in cleartext in a memory location that
might be accessible to actors other than the intended user. Includes heap memory not zeroed
before deallocation, and stack frames not zeroed before reuse.

**Platform context (two variants)**:

**Variant A — Heap string on rejection path**: `String::drop()` does NOT zero the heap allocation.
A rejected token string remains readable in heap memory until the allocator reuses those bytes.
Discovered: ENG-KHADAS-AUDIT-20260522 (F-4).

```rust
// WRONG — String::drop() does not zero heap bytes
.filter(|t| t.len() >= MIN_TOKEN_LEN)
.map(|t| SecretBox::new(Box::new(t)))

// CORRECT — explicit zeroize before drop on rejection path
.and_then(|mut t| {
    if t.len() < MIN_TOKEN_LEN {
        zeroize::Zeroize::zeroize(&mut t);  // zero heap before drop
        return None;
    }
    Some(SecretBox::new(Box::new(t)))
})
```

**Variant B — Stack comparison buffer**: A `[u8; N]` buffer filled with token bytes for `ct_eq()`
holds the token fragment on the stack until the stack frame is reused. Discovered in-gate during
ENG-KHADAS-AUDIT-20260522 S1 review (S1-R1).

```rust
// WRONG — [u8; N] stack buffer holds token fragment until stack reuse
let mut padded = [0u8; 4];

// CORRECT — Zeroizing<[u8; N]> auto-zeroes on drop
let mut padded = Zeroizing::new([0u8; 4]);
```

**Platform crates**:
- `zeroize` (workspace dep) — `Zeroize` trait + `Zeroizing<T>` wrapper
- `secrecy` (workspace dep) — `SecretString`, `SecretBox<T>` for heap-allocated secrets
- Do NOT use `SecretString` for stack buffers — it wraps a `String`, not `[u8; N]`

**Severity** (platform classification):
- Variant A (heap rejection path): HIGH when token is admin-level credential
- Variant B (stack comparison buffer): MEDIUM

**Gate check ([S])**: any local variable or stack buffer containing secret bytes without
`Zeroizing<T>` wrapper is a S1-class finding. `SecretString` and `SecretBox` are NOT sufficient
for stack-allocated comparison buffers.
