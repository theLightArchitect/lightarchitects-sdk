---
title: iter-9 SCRUM tracked findings — HIGH severity, not folded per Blueprint §14.3
codename: gitforest-live-ops
iteration: 9
scrum_round: post-reconstruction validation
reviewed_at: 2026-05-18T12:30:00Z
classification_authority: "Architects Blueprint §14.3 two-tier amendment classification"
fold_status: tracked-not-folded (HIGH severity)
fold_target: "/GATE-3 prep OR /GATE-7 pre-merge"
---

# iter-9 SCRUM HIGH Findings Track Record

Per `memory://feedback_two_tier_amendment_classification`: HIGH findings do NOT gate VALIDATED;
they are tracked here for /BUILD executor reference and folded at the next /GATE prep where
the impact lands. Plan body stays clean of HIGH-tier bloat per Blueprint §14.3.

## H1 [quality lens] — Per-phase Northstar fit predicate missing (C7c)

**Severity**: HIGH | **Owner**: engineer | **Fold target**: /GATE-3 prep (Phase 3 exit criterion)

Plan body Part VIII per-phase exit criteria list quality gates (cargo, svelte-check, test counts)
without explicit Pillar-progression predicate. Part XIV claims C6=95 citing "per-phase Northstar
fit predicate" but grep returns 0 hits for `fit_predicate` / `northstar_fit` / `c7c`.

**Fix at /GATE-3 prep**: add `northstar_fit:` line to each phase exit:
- Phase 1 → `P4 schema lock: BranchKind enum matches 4-level hierarchy; types compile`
- Phase 2 → `P2 H5 mechanical: AYIN span → halo pulse ≤500ms verified empirically (smoke test)`
- Phase 3 → `P2 60fps + LOD: 200-node cap held at 30 ev/s for 60s`
- Phase 3.5 → `P1 a11y: axe-core 0 violations + keyboard tab through 12 branches`
- Phase 4 → `P4 + Security: 3 endpoints respond 200 + SERAPH pentest PASS`
- Phase 5 → `P1 click-to-route L3: TaskDrillView renders within 500ms of click`
- Phase 6 → `P4 ≤8s triage: operator timing sample geomean ≤8s`
- Phase 7 → `P1+P2+P4 final mechanical via 8 HIGH E2E scenarios green`

## H2 [quality lens] — `[C]` Canon gate absent from Part III gate table

**Severity**: HIGH | **Owner**: knowledge | **Fold target**: /GATE-4 prep

Part III gate-vocabulary table advertises `[A+S+Q+C+O+P+K+D+T+R]` but per-phase column lists NO
`[C]` entry on Phases 1-7. LÆX0 owns `[C]` per Gatekeeper Registry.

**Fix at /GATE-4 prep**: add `[C]` to Phase 4 gates (security canon cross-check — Cookbook §63 +
Security-Guardrails §6.1) and Phase 7 gates (full canon cross-check across 7 docs).

## H3 [knowledge lens] — FFM #27a/27b/27c sub-letters referenced but Part VI is flat

**Severity**: HIGH | **Owner**: engineer | **Fold target**: /GATE-7 docs review

Body cites "#27a-c knowledge owner across 3 paths" and "5a/5b/5c/5d" sub-rows. Part VI table is
flat 1-38 (now 1-37 + 36b after iter-9 C1 fold).

**Fix at /GATE-7 prep**: backfill sub-letter splits in Part VI table OR remove sub-letter
citations from body. Current state has cosmetic consistency loss but functionally correct
(each file IS in the FFM, just not under sub-letter).

## H4 [security lens] — Auth model not explicitly declared

**Severity**: HIGH (now folded via iter-9 C3 — `auth_model_explicit` field added) | **Status**: PARTIALLY RESOLVED

Iter-9 C3 fold added `auth_model_explicit: browser AuthGuard` to security_compliance block.
Remaining gap: Phase 4 task spec (FFM #5) doesn't explicitly cite AuthGuard reuse vs
X-LA-Notify-Token machine-token (per memory://feedback_webshell_auth_model_split).

**Fix at /GATE-4 prep**: FFM #5 routes/gitforest.rs task description should explicitly state
"reuse AuthGuard from existing /api/* routes; X-LA-Notify-Token is for machine-to-machine only".

## H5 [security lens] — `seraph_pentest_for_critical_paths` field

**Status**: RESOLVED via iter-9 C3 fold — field added to security_compliance block.

## H6 [researcher lens] — DC-6/DC-7 RESEARCH-BACKED claims lack resolvable citations

**Severity**: HIGH | **Owner**: researcher | **Fold target**: /GATE-7 docs review

Part XIX.A DC-6 (4Hz cadence) + DC-7 (2.5s decay) declared `RESEARCH-BACKED` citing "Stream C §4.2".
Stream C lists 9 tools but provides no URL/doc reference for Datadog cadence or HCI eye-fixation
research claims. Per Canon XXXV inline citation gate + `feedback_inline_citation_protocol`,
RESEARCH-BACKED tier needs resolvable `[N]` IEEE refs.

**Fix at /GATE-7 prep**: either (a) add inline `[1] Datadog Service Map docs URL` + `[2]
perceptual HCI source` to Stream C and Part XIX bibliography, OR (b) downgrade DC-6/DC-7 to
"design-choice with falsifiability trigger" matching DC-2/DC-3 tier.

## H7 [researcher lens] — Part XXIII SCRUM record under-attributed

**Severity**: HIGH | **Owner**: knowledge | **Fold target**: /GATE-7 docs review

Part XXIII §1 has 12-finding R1 table fully attributed (each row maps to a sibling SCR ID).
Part XXIII §2 R2/R3 has 31 findings with per-finding attribution (CO-R2-N, SE-R2-N, etc.) but
no consolidated table mapping all to siblings + dispositions + R3 verdict-delta. Per
`memory://feedback_scrum_r3_verdict_upgrade_signature`, the convergence proof needs explicit
3+/7 upgrade arithmetic.

**Fix at /GATE-7 prep**: insert R1/R2/R3 consolidated table with columns: Finding ID | Lens |
Severity | Disposition (FOLD/LABEL/DEFER) | R3 verdict-delta (UPGRADE/DOWNGRADE/HOLD).

---

## Synthesis

7 HIGH items tracked. 2 already partially resolved by iter-9 C3 fold (H4, H5). 5 remain for
/GATE-3, /GATE-4, or /GATE-7 prep stages. None gate VALIDATED status; all are calibration
polish or documentation completeness.

**Per Blueprint §14.3**: this record is the *track-in-review-record* tier. Plan body stays
clean. /BUILD executor consults this file when entering each /GATE prep phase.

---

## Appendix A — Context7 Library Reference (Phase 3 + Phase 4 + Phase 6 implementer)

Fetched 2026-05-18T13:30Z via `mcp__plugin_context7_context7__query-docs`. Cached here as primary anchor for /BUILD implementers; resolves Canon XXXV inline-citation requirement.

### A.1 Three.js (`/websites/threejs`) — Phase 3 critical

```rust
// Frustum.intersectsObject — REQUIRES geometry.boundingSphere
// THREE.Group has NO geometry → cull per-Mesh inside renderNode
frustum.intersectsObject(mesh)  // ✓ valid
frustum.intersectsObject(group) // ✗ broken — no geometry

// InstancedMesh — auto-bounding sphere recomputed by engine for culling
const inst = new THREE.InstancedMesh(geo, mat, maxCount)
inst.setMatrixAt(i, matrix)
inst.instanceMatrix.needsUpdate = true
inst.computeBoundingSphere()  // call after setMatrixAt batch

// LineMaterial — fat lines (LineBasicMaterial wireframeLinewidth always 1px)
import { LineMaterial } from 'three/examples/jsm/lines/LineMaterial.js'
import { LineSegments2 } from 'three/examples/jsm/lines/LineSegments2.js'
import { LineGeometry } from 'three/examples/jsm/lines/LineGeometry.js'
const mat = new LineMaterial({ color, linewidth: 2, worldUnits: false })
```

### A.2 web-vitals v4 (`/googlechrome/web-vitals`) — Phase 6 (FFM #5d)

```typescript
import { onLCP, onINP, onCLS, onTTFB, LCPThresholds, INPThresholds } from 'web-vitals';

// Callback-based API (NOT polling)
onLCP((metric) => {
  // metric: { name, value (ms), rating, delta, id, entries: PerformanceEntry[], navigationType }
  ayinReport({ metric: 'lcp', value: metric.value, rating: metric.rating });
});

onINP(callback, { durationThreshold: 16, reportAllChanges: true });
onCLS(callback);  // monitors continuously
onTTFB(callback); // fires once after load

// Thresholds (use for rating UI):
// LCPThresholds = [2500, 4000]  ms — good/needs-improvement/poor
// INPThresholds = [200, 500]    ms
```

### A.3 moka 0.12 (`/moka-rs/moka`) — Phase 4 (FFM #5 github_proxy.rs)

```rust
// Cargo.toml: moka = { version = "0.12", features = ["future"] }
use moka::future::Cache;
use std::time::Duration;

let cache = Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(60))  // 60s TTL per plan
    .build();

// async insert/get
cache.insert(key, value).await;
let value = cache.get(&key).await;
cache.invalidate(&key).await;

// get_with: single-flight stampede protection (concurrent calls to missing key coalesce)
let value = cache.get_with(key, async {
    fetch_from_github(repo, sha).await  // only ONE task evaluates; others wait
}).await;

// Clone cache for sharing — cheap operation, NO Arc<Mutex<>>
let cache_clone = cache.clone();
tokio::spawn(async move { cache_clone.insert(k, v).await; });

// Eviction listener (optional — for AYIN broadcast on eviction)
let cache = Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(60))
    .eviction_listener(move |key, value, cause| {
        tracing::debug!(target: "github_proxy.eviction", ?key, ?cause);
    })
    .build();
```

### A.4 xxhash-rust 0.8 (`/doumanash/xxhash-rust`) — Phase 3 (cluster_hash AY-R2-1)

```rust
// Cargo.toml: xxhash-rust = { version = "0.8", features = ["xxh32"] }
use xxhash_rust::xxh32::xxh32;

// Runtime 32-bit hash
let cluster_hash = xxh32(branch_ids_sorted_bytes, 0) % 50;  // 50-bucket cardinality cap

// Compile-time variant (for static constants)
use xxhash_rust::const_xxh32::xxh32 as const_xxh32;
const POLYTOPE_KIND_PENTACHORON: u32 = const_xxh32(b"pentachoron", 0);

// Streaming variant (for incremental hashing of dynamic input)
use xxhash_rust::xxh32::Xxh32;
let mut hasher = Xxh32::new(0);
hasher.update(b"chunk1");
hasher.update(b"chunk2");
let hash = hasher.digest();
```

### A.5 idb 8.x (`/jakearchibald/idb`) — Phase 1 (FFM #5c gitforestCache.ts; already shipped)

Already implemented. Verify against current shipped code:
- `openDB('gitforest-cache', 1, { upgrade(db) { db.createObjectStore('topology', { keyPath: 'repo' }); } })`
- `db.transaction('topology', 'readwrite').store.put(value, key)`
- `db.transaction('topology', 'readonly').store.get(key)`
- SWR pattern: return stale immediately, fetch fresh in background, update cache on success

---

**Phase 3+ implementer instruction**: cite this Appendix A section in JSDoc/rustdoc when implementing any of the 5 deps. Cache is fresh as of 2026-05-18T13:30Z; if /BUILD launches >14d after this date, re-fetch via context7 per memory://feedback_pre_completion_during_plan_authoring staleness rule.
