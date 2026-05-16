#!/usr/bin/env python3
"""
Squad Synthesizer — fan-in from N gate_evaluation blocks → squad_review.yaml.

Spec: canon/squad-synthesizer-protocol.md
Schema: canon/lasdlc-spec.md §4.5
Authority: canon/gatekeeper-registry.yaml

Exit codes:
  0  PASS / GAPS_NOTED
  1  FIXES_REQUIRED
  2  FAIL / VALIDATION_ERROR
  3  Invocation error
"""
from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean
from typing import Any

try:
    import yaml
except ImportError:
    print("ERROR: pyyaml required — pip install pyyaml", file=sys.stderr)
    sys.exit(3)

SCHEMA_VERSION = "v1.0"
# 9 dimensions: [A][S][Q][C][O][P][K][D][T][R]
# Per gatekeeper-registry.yaml v1.1 and plugins commit c06c807
VALID_GATES = {"[A]", "[S]", "[Q]", "[C]", "[P]", "[T]", "[D]", "[O]", "[K]", "[R]"}
# [C] Canon veto added (LÆX0 lens via quality agent)
VETO_GATES = {"[S]", "[K]", "[C]"}
REQUIRED_QUORUM = {"[S]", "[K]"}   # [R] is expected but non-quorum (warning only)
GAPS_TIER = {0: "PASS", 1: "GAPS_NOTED", 2: "GAPS_NOTED"}

GATE_TO_INVOCATION = {
    "[A]": "lightarchitects:engineer",
    "[S]": "lightarchitects:security",
    "[Q]": "lightarchitects:quality",
    "[C]": "lightarchitects:quality",    # LÆX0 canon enforcement lens within quality agent
    "[P]": "lightarchitects:ops",
    "[T]": "lightarchitects:testing",
    "[D]": "lightarchitects:knowledge",
    "[O]": "lightarchitects:ops",
    "[K]": "lightarchitects:knowledge",
    "[R]": "lightarchitects:researcher",
}

GATE_TO_SIBLING = {
    "[A]": "CORSO",
    "[S]": "SERAPH",
    "[Q]": "CORSO",
    "[C]": "LÆX0",
    "[O]": "EVA",
    "[P]": "EVA",   # primary; AYIN provides observability lens
    "[K]": "SOUL",
    "[D]": "SOUL",
    "[T]": "CORSO",
    "[R]": "QUANTUM",
}


def gaps_tier(count: int, has_fail: bool) -> str:
    if has_fail:
        return "FIXES_REQUIRED"
    if count == 0:
        return "PASS"
    if count <= 2:
        return "GAPS_NOTED"
    return "FIXES_REQUIRED"


def validate_block(block: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    ge = block.get("gate_evaluation", block)

    if ge.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be '{SCHEMA_VERSION}', got {ge.get('schema_version')!r}")

    gate = ge.get("gate")
    if gate not in VALID_GATES:
        errors.append(f"gate {gate!r} not in valid set {VALID_GATES}")

    for field in ("scored_by", "scored_at", "build_id", "phase_id", "overall_verdict"):
        if not ge.get(field):
            errors.append(f"missing required field: {field}")

    overall = ge.get("overall_verdict", "")
    if overall not in ("PASS", "GAPS", "FAIL"):
        errors.append(f"overall_verdict must be PASS|GAPS|FAIL, got {overall!r}")

    # Canon XXXV: GAPS/FAIL scored anchors must have verbatim_anchor_quote
    for anchor in ge.get("scored_anchors", []):
        verdict = anchor.get("verdict", "")
        if verdict in ("GAPS", "FAIL") and not anchor.get("verbatim_anchor_quote"):
            errors.append(
                f"CITATION_VIOLATION: anchor {anchor.get('uuid', '?')} has verdict={verdict} "
                "but missing verbatim_anchor_quote (Canon XXXV)"
            )

    # invocation consistency — multi-gate agents are allowed on their paired gates
    expected_inv = GATE_TO_INVOCATION.get(gate)
    if expected_inv and ge.get("scored_by") not in (expected_inv, None):
        scored_by = ge.get("scored_by")
        # ops covers [O] and [P]; quality covers [Q] and [C] (LÆX0 lens)
        allowed = (
            (gate in ("[O]", "[P]") and scored_by == "lightarchitects:ops")
            or (gate in ("[Q]", "[C]") and scored_by == "lightarchitects:quality")
            or (gate in ("[K]", "[D]") and scored_by == "lightarchitects:knowledge")
        )
        if not allowed:
            errors.append(
                f"scored_by {scored_by!r} does not match expected {expected_inv!r} for gate {gate}"
            )

    return errors


def merge_ldb(blocks: list[dict]) -> dict[str, dict]:
    buckets: dict[str, list[dict]] = {}
    for block in blocks:
        ge = block.get("gate_evaluation", block)
        for comp in ge.get("ldb_components", []):
            cid = comp.get("id")
            interval = comp.get("interval", {})
            if cid and interval:
                buckets.setdefault(cid, []).append(interval)

    result = {}
    for cid, intervals in buckets.items():
        lows = [i.get("low", 0) for i in intervals if "low" in i]
        points = [i.get("point", 0) for i in intervals if "point" in i]
        highs = [i.get("high", 0) for i in intervals if "high" in i]
        result[cid] = {
            "low": min(lows) if lows else None,
            "point": round(mean(points)) if points else None,
            "high": max(highs) if highs else None,
        }
    return result


def detect_disputes(blocks: list[dict]) -> list[dict]:
    uuid_map: dict[str, dict[str, str]] = {}
    for block in blocks:
        ge = block.get("gate_evaluation", block)
        scored_by = ge.get("scored_by", "unknown")
        for anchor in ge.get("scored_anchors", []):
            uid = anchor.get("uuid")
            verdict = anchor.get("verdict")
            if uid and verdict:
                uuid_map.setdefault(uid, {})[scored_by] = verdict

    disputes = []
    for uid, verdicts_by_agent in uuid_map.items():
        unique = set(verdicts_by_agent.values())
        if len(unique) > 1:
            disputes.append({
                "anchor_uuid": uid,
                "scoring_agents": verdicts_by_agent,
                "resolution": "pending",
            })
    return disputes


def synthesize(build_root: Path, phase_id: str, validate_only: bool = False) -> int:
    gate_evals_dir = build_root / ".gate-evals"
    if not gate_evals_dir.exists():
        print(f"ERROR: .gate-evals/ not found at {gate_evals_dir}", file=sys.stderr)
        return 3

    eval_files = sorted(gate_evals_dir.glob(f"{phase_id}-*.yaml"))
    if not eval_files:
        print(f"ERROR: no gate_evaluation files found for phase {phase_id!r}", file=sys.stderr)
        return 3

    blocks: list[dict] = []
    all_errors: list[dict] = []
    build_ids: set[str] = set()

    for fp in eval_files:
        raw = yaml.safe_load(fp.read_text())
        if raw is None:
            all_errors.append({"file": str(fp), "errors": ["empty file"]})
            continue
        ge = raw.get("gate_evaluation", raw)
        errs = validate_block(raw)
        if errs:
            all_errors.append({"file": str(fp), "gate": ge.get("gate"), "errors": errs})
        else:
            blocks.append(raw)
            build_ids.add(ge.get("build_id", ""))

    # cross-block build_id consistency
    if len(build_ids) > 1:
        all_errors.append({"file": "cross-block", "errors": [f"inconsistent build_ids: {build_ids}"]})

    if all_errors:
        if not validate_only:
            out = build_root / ".squad" / "squad-review.yaml"
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(yaml.dump({
                "squad_review": {
                    "schema_version": SCHEMA_VERSION,
                    "phase_id": phase_id,
                    "synthesized_at": datetime.now(timezone.utc).isoformat(),
                    "verdict": "VALIDATION_ERROR",
                    "errors": all_errors,
                }
            }, default_flow_style=False, sort_keys=False))
        print(f"VALIDATION_ERROR: {len(all_errors)} block(s) failed validation", file=sys.stderr)
        for e in all_errors:
            for msg in e.get("errors", []):
                print(f"  [{e.get('gate', e.get('file', '?'))}] {msg}", file=sys.stderr)
        return 2

    if validate_only:
        print(f"OK: {len(blocks)} block(s) validated for phase {phase_id!r}")
        return 0

    # --- Step 2: veto application ---
    # Collect ALL vetoes (security, knowledge, canon); don't stop at first
    vetoes: list[dict] = []
    citation_violations = any(
        "CITATION_VIOLATION" in err
        for e in all_errors
        for err in e.get("errors", [])
    )
    for block in blocks:
        ge = block.get("gate_evaluation", block)
        gate = ge.get("gate")
        if gate in VETO_GATES and ge.get("overall_verdict") == "FAIL":
            vetoes.append({
                "authority": ge.get("scored_by"),
                "sibling": GATE_TO_SIBLING.get(gate, "unknown"),
                "gate": gate,
                "reason": f"overall_verdict FAIL — {gate} gatekeeper ({GATE_TO_SIBLING.get(gate, '?')})",
            })
    if citation_violations:
        vetoes.append({
            "authority": "lightarchitects:knowledge",
            "sibling": "SOUL",
            "gate": "[K]",
            "reason": "CITATION_VIOLATION: verbatim_anchor_quote missing on GAPS/FAIL anchor (Canon XXXV)",
        })
    veto_applied = vetoes if vetoes else None

    # --- Step 3: GAPS counting ---
    # Per verdict_aggregation table: "FAIL if any gate fails."
    # Scoring priority: scored_anchors drive the count when populated (granular).
    # When scored_anchors is empty, overall_verdict is the signal (non-veto gates only;
    # veto gates are already handled in Step 2).
    gaps_count = 0
    has_fail = False
    gate_verdicts: dict[str, str] = {}
    for block in blocks:
        ge = block.get("gate_evaluation", block)
        gate = ge.get("gate")
        ov = ge.get("overall_verdict", "UNKNOWN")
        gate_verdicts[gate] = ov
        anchors = ge.get("scored_anchors", [])
        if anchors:
            # scored_anchors present — they drive the count
            for anchor in anchors:
                v = anchor.get("verdict", "")
                if v == "FAIL":
                    has_fail = True
                    gaps_count += 1
                elif v == "GAPS":
                    gaps_count += 1
        elif gate not in VETO_GATES:
            # no scored_anchors; fall back to overall_verdict for non-veto gates
            if ov == "FAIL":
                has_fail = True
            elif ov == "GAPS":
                gaps_count += 1

    tier = gaps_tier(gaps_count, has_fail)

    # --- quorum check ---
    present_gates = set(gate_verdicts.keys())
    missing_required = REQUIRED_QUORUM - present_gates
    if missing_required:
        if veto_applied is None:
            veto_applied = []
        veto_applied.append({
            "authority": "synthesizer",
            "sibling": "synthesizer",
            "gate": sorted(missing_required),
            "reason": f"Required gatekeepers absent: {sorted(missing_required)} (quorum rule §4 of synthesizer protocol)",
        })
        tier = "FAIL"

    final_verdict = "FAIL" if veto_applied else tier

    # --- Step 4: dispute detection ---
    disputes = detect_disputes(blocks)

    # --- Step 5: LDB aggregation ---
    ldb_aggregate = merge_ldb(blocks)

    # missing gatekeepers — [R] is expected but non-quorum; others warn
    all_gates = set(gate_verdicts.keys())
    missing_all = VALID_GATES - all_gates
    missing_non_required = missing_all - REQUIRED_QUORUM
    missing_warnings = []
    if "[R]" in missing_non_required:
        missing_warnings.append("[R] researcher (QUANTUM) absent — risk assessment skipped")

    consumed = []
    for fp in eval_files:
        ge = yaml.safe_load(fp.read_text()).get("gate_evaluation", {})
        consumed.append({
            "file": str(fp.relative_to(build_root)),
            "scored_by": ge.get("scored_by"),
            "scored_at": ge.get("scored_at"),
        })

    build_id = next(iter(build_ids), "unknown")
    review: dict[str, Any] = {
        "squad_review": {
            "schema_version": SCHEMA_VERSION,
            "build_id": build_id,
            "phase_id": phase_id,
            "synthesized_at": datetime.now(timezone.utc).isoformat(),
            "verdict": final_verdict,
            "gate_verdicts": gate_verdicts,
            "gaps_count": gaps_count,
            "gaps_threshold_tier": tier,
            "veto_applied": veto_applied,
            "disputes": disputes,
            "ldb_aggregate": ldb_aggregate,
            "consumed_evals": consumed,
            "missing_gatekeepers": sorted(missing_non_required),
            "missing_warnings": missing_warnings,
        }
    }

    out = build_root / ".squad" / "squad-review.yaml"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(yaml.dump(review, default_flow_style=False, sort_keys=False))

    print(f"squad_review verdict: {final_verdict}  (gaps={gaps_count}, disputes={len(disputes)})")
    print(f"Written: {out}")

    return 0 if final_verdict in ("PASS", "GAPS_NOTED") else 1 if final_verdict == "FIXES_REQUIRED" else 2


def main() -> None:
    parser = argparse.ArgumentParser(description="Squad Synthesizer — aggregate gate_evaluation blocks")
    parser.add_argument("--phase", required=True, help="Phase ID (matches .gate-evals/<phase-id>-*.yaml)")
    parser.add_argument("--build-root", required=True, type=Path, help="Build root directory")
    parser.add_argument("--validate-only", action="store_true", help="Validate inputs without writing output")
    args = parser.parse_args()

    sys.exit(synthesize(args.build_root, args.phase, args.validate_only))


if __name__ == "__main__":
    main()
