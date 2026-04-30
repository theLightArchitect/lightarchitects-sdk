# ADR-006: Content-Security-Policy Rollout for the Webshell

**Status**: Accepted
**Date**: 2026-04-30
**Build**: unifying-rolling-aegis (Wave 2 — SECURITY_HARDEN)
**Task IDs**: #65 SEC-3a (Report-Only), #66 SEC-3b (Enforce flip), #70 (this ADR)

## Context

The `lightarchitects-webshell` serves a Svelte SPA over HTTP at
`http://127.0.0.1:<port>`. The browser running the SPA holds a long-lived
Bearer token in `localStorage` and uses it to authenticate SSE streams and
SOUL/coordination API calls. An XSS exploit in any third-party
component, a stale CDN script, or even a careless `innerHTML` would let an
attacker exfiltrate that token and impersonate the operator end-to-end.

CORS on its own is insufficient — it controls which origins may *read*
responses, not which scripts the browser is willing to *execute*. CSP
(Content-Security-Policy) is the canonical defense-in-depth layer for the
"my own page got compromised" failure mode.

Two questions must be answered before turning CSP on:

1. **Will it break the app?** A wrong directive can disable Svelte hydration,
   inline event handlers, or third-party fonts. We must learn what breaks
   *before* we block requests.
2. **What does the policy need to allow?** Production runs `rust_embed`'d
   bundled assets on a single origin; Vite dev mode runs the SPA on
   `:5173` with HMR. The two modes have wildly different surfaces.

## Decision

CSP rolls out in two staged phases. Each is its own task in the aegis manifest:

### Phase 1 — Report-Only (task #65, SEC-3a)

Set `Content-Security-Policy-Report-Only` on every HTTP response.
Browsers evaluate the policy and emit `report-to` violations to a backend
endpoint, but **do not block** any requests. This phase observes-only.

The release policy:

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
connect-src 'self' ws://127.0.0.1:* http://127.0.0.1:*;
img-src 'self' data:;
font-src 'self' data:;
frame-ancestors 'none';
base-uri 'self';
form-action 'self';
object-src 'none';
report-uri /api/csp-report;
```

A violation collector at `POST /api/csp-report` ingests `application/csp-report`
bodies, logs them via `tracing::warn!(target: "csp", ...)`, and counts
unique violation patterns. After 7 days of operator usage with zero novel
violations, we move to Phase 2.

### Phase 2 — Enforce flip (task #66, SEC-3b)

Swap the response header from `Content-Security-Policy-Report-Only` to
`Content-Security-Policy`. Same policy text — only the header name changes.
Browsers now block any request that violates the policy.

A `--csp-mode=report|enforce|off` runtime flag (defaulting to `enforce`)
keeps an emergency rollback available without redeploy.

## Rationale

1. **Report-Only first means no user-visible breakage.** If the policy is
   wrong (e.g. a third-party font we forgot to allow), violations show up
   in `csp-report` logs without users seeing white screens or broken
   features. We learn what's wrong before we make it visible.
2. **Two-task split lets us pause between phases.** The aegis manifest
   commits to landing #65 first in Wave 2 and #66 last in Wave 2, with
   the rest of the wave in between. That gives ~3 days of report-only
   exposure before the enforce flip.
3. **`'unsafe-inline'` for `style-src` is a known concession.** Svelte 5's
   reactive `style:` directives compile to inline style attributes. The
   alternative — nonces or hashes per-render — adds non-trivial complexity
   and breaks SSR-less hydration. Inline styles in a CSP context grant only
   styling capability, which has no exfil primitive on its own.
4. **`script-src 'self'` (no `'unsafe-inline'`, no `'unsafe-eval'`) is
   strict.** Bundled Vite output is fully external; we never need eval.
   Any future feature that wants to evaluate dynamic JS needs an explicit
   ADR amendment.
5. **`connect-src` allows `ws://127.0.0.1:*` and `http://127.0.0.1:*`** to
   permit the WebSocket terminal upgrade and same-origin SSE without
   special-casing each port the user might bind to.
6. **`frame-ancestors 'none'`** blocks clickjacking outright. The webshell
   has no embedding use case.

## Alternatives Considered

- **Skip CSP, rely on token rotation + short-lived tokens.** Rejected:
  long-lived tokens are the existing model (single-user, local install),
  and rotating them on every action would break SSE continuity. CSP is
  the cheaper, defense-in-depth fix.
- **Use a strict policy with nonces.** Rejected for v0.3: nonce plumbing
  through `rust_embed` + Vite + Svelte 5 is non-trivial and would slip
  the rollout. Revisit in v0.4 (task #74-adjacent).
- **Apply CSP only to the SPA HTML, not API responses.** Rejected: CSP
  on JSON responses is harmless (browsers ignore it for XHR), and a
  single middleware that sets the header on every response is simpler
  than per-route conditional logic.

## Consequences

### Positive

- An XSS in any Svelte component or third-party dep can no longer
  exfiltrate the Bearer token: external `connect-src` is denied.
- Inline `<script>` injection (the most common XSS vector) is blocked
  outright by `script-src 'self'`.
- Clickjacking is impossible (`frame-ancestors 'none'`).
- `csp-report` logs become an early-warning signal for compromised deps.

### Negative

- `'unsafe-inline'` in `style-src` is a known weakness — it lets injected
  HTML set arbitrary styles. Callers who care about pixel-perfect
  brand-impersonation defense should upgrade to a nonce-based policy
  (deferred to v0.4 per ADR amendment if needed).
- Vite dev mode will need a separate, looser policy. The release build's
  policy will reject Vite's HMR `eval`'d hot-reload code. Approach: emit
  the strict policy *only* when the server is bound to non-`5173`
  origins, OR use `cfg(debug_assertions)` to relax `script-src` to
  `'self' 'unsafe-eval' 'unsafe-inline'` in dev builds. The dev policy
  is documented but never shipped to operators.
- A dep update that introduces inline JS will fail silently in
  Report-Only mode, then break loudly in Enforce mode. Mitigation:
  monitor `csp-report` logs after every dep bump.

### Operational

- `make ci-squad-local` is extended to grep release HTML for
  `<script>` tags without `src=` (catches inline JS regressions before
  CSP catches them in production).
- `--csp-mode` flag added to `lightarchitects-webshell start` so an
  operator can hot-disable enforcement without rebuild if a future dep
  bump trips the policy unexpectedly.

## References

- Aegis manifest: `~/lightarchitects/soul/helix/corso/builds/unifying-rolling-aegis/manifest.yaml` (Wave 2)
- MDN CSP: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Security-Policy
- W3C CSP3: https://www.w3.org/TR/CSP3/
- Builders Cookbook §32 (defense-in-depth, no-eval directive)
