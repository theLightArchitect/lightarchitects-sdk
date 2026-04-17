# Phase 1.5 Supply-Chain Audit — web-figma/ dependencies

**Build**: luminous-grafting-nautilus
**Date**: 2026-04-16
**Source**: `pnpm --dir web-figma audit`
**Decision**: Document + proceed, revisit at Phase 7 (Kevin, 2026-04-16)
**Risk tier**: dev-only, local-only, accepted for baseline

---

## Production-dependency audit

**Finding**: No known vulnerabilities.

```
$ pnpm --dir web-figma audit --prod
No known vulnerabilities found
```

The shipped binary embeds `web-figma/dist/*` via `rust-embed` and runs zero JavaScript outside that bundle. Production deps are clean.

---

## Development-dependency audit

**Finding**: 5 vulnerabilities, all in `vite@6.3.5` (the version pinned by Figma Make).

| Severity | Advisory | Title | Patched |
|---------:|----------|-------|---------|
| HIGH     | [GHSA-p9ff-h696-f583](https://github.com/advisories/GHSA-p9ff-h696-f583) | Vite Vulnerable to Arbitrary File Read via Vite Dev Server WebSocket | ≥ 6.4.2 |
| MODERATE | [GHSA-93m4-6634-74q7](https://github.com/advisories/GHSA-93m4-6634-74q7) | vite allows `server.fs.deny` bypass via backslash on Windows | ≥ 6.4.1 |
| MODERATE | [GHSA-4w7w-66w2-5vf9](https://github.com/advisories/GHSA-4w7w-66w2-5vf9) | Vite Vulnerable to Path Traversal in Optimized Deps `.map` Handling | ≥ 6.4.2 |
| LOW      | [GHSA-g4jq-h2w9-997c](https://github.com/advisories/GHSA-g4jq-h2w9-997c) | Vite middleware may serve files starting with the same name as public dir | ≥ 6.3.6 |
| LOW      | [GHSA-jqfw-vq24-v9c3](https://github.com/advisories/GHSA-jqfw-vq24-v9c3) | Vite's `server.fs` settings not applied to HTML files | ≥ 6.3.6 |

---

## Threat model (why dev-only is accepted for this baseline)

1. **Production exposure: NONE.** The lightarchitects-webshell binary uses `rust-embed` to bake `web-figma/dist/*` at compile time. Vite is never invoked in the shipped binary — it is a build-time tool only. Arbitrary file read through `vite` dev-server WebSocket cannot be reached from a running webshell.

2. **Dev-server exposure: local-loopback only.** Vite's dev server binds to `127.0.0.1` by default. Any exploit against `ws://localhost:<port>` requires a malicious process already running on the developer's laptop with network access to loopback. This is a post-compromise privilege-escalation vector, not a remote attack.

3. **Windows backslash bypass: N/A.** Primary development machine is macOS (Darwin 25.1.0). The Windows-specific MODERATE does not apply.

4. **.map path traversal: limited.** Serves .map files from optimized-deps cache. Exposure is the contents of `node_modules` already on disk — not novel information.

5. **Public-dir prefix confusion: low impact.** Serves files starting with public/ prefix names outside public/. Scoped to development build.

---

## Why not patch now

Per plan §0d_architecture_decisions: **Figma Make owns `package.json` pinning**. The consolidated `web-figma/package.json` is first-party Figma Make source-of-truth. Modifying it pre-discovery would:

1. **Pollute the baseline** before the Phase 1.5 Step-6 write-path experiment. If Figma Make re-syncs `package.json` on next publish, my override vanishes and we learn nothing about whether Figma writes to build config.
2. **Violate the partition premise** (A.2 sibling-folder decision): Figma territory is authoritative. Engineering edits in Figma territory are the precise failure mode we are guarding against.
3. **Invite churn** if Figma Make's next sync happens to bump Vite independently — we'd be fighting a moving target.

The clean resolution path is for Figma Make to publish a new `package.json` with `vite >= 6.4.2`. Kevin's action, not ours. Unknown whether Figma Make exposes devDep version control to designers; this is an open question for Step 5's probe.

---

## Deferred action — Phase 7 SCRUM agenda

When Phase 7 runs:

- [ ] Re-run `pnpm --dir web-figma audit` to capture any version drift from intermediate Figma Make syncs
- [ ] If Vite still < 6.4.2 and Figma Make cannot bump it:
   1. Investigate `pnpm.overrides` as a Figma-Make-preserving mechanism (note: overrides live in `package.json`, which is Figma territory — this is the fallback option only if Figma sync is proven to leave overrides alone)
   2. Alternatively, pin via `pnpm-workspace.yaml` or `.npmrc` directives (if Figma does not write to those)
- [ ] File SERAPH finding in the exit gate triage if vulnerability severity changes (e.g., HIGH → CRITICAL)
- [ ] Confirm the threat model still holds (webshell remains local-dev-only)

---

## Reference — CLAUDE.md policy position

`~/.claude/CLAUDE.md` coding_guidelines enforcement block:

> `cargo audit`: block merge on CVE

The policy is written for Rust supply chain. It is silent on:
- pnpm audit (frontend)
- dev-vs-prod distinction
- local-dev-only tool tier

For this build, the interpretation applied is: **production-dependency audit must be clean (is clean), development-dependency audit findings are tracked and remediated at SCRUM**. This aligns with Canon V (calculated confidence) and Canon X (cost of the tower — no spending on speculative patches that Figma Make might revert).

---

## Kevin's rationale (verbatim, 2026-04-16)

> "Accurate Threat Modeling: These are devDependency vulnerabilities in Vite. They do not ship to your production bundle. The risk is entirely scoped to your local development environment or build pipeline. For a Phase 1.5 baseline, this is an acceptable, non-critical risk.
>
> Protects the Experiment: You haven't done the write-path discovery yet. If you try pnpm overrides, you are polluting the baseline package.json before you even know if Figma Make will aggressively overwrite it on the next sync. This could create false positives in your discovery phase.
>
> Figma Make Constraints: Fighting its internal version pinning right now is a distraction from your primary goal: establishing the split-pane integration boundary.
>
> Maintains Momentum with Integrity: By writing it down in docs/phase-1.5-supply-chain.md, you explicitly acknowledge the tech debt and schedule its resolution for a phase where your architecture is actually stable enough to handle it."

---

*Commit reference*: created before Phase 1.5 Step 4 baseline commit so the supply-chain record is in the same git history as the consolidated tree.
