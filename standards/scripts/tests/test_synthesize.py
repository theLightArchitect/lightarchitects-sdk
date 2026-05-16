#!/usr/bin/env python3
"""
Test suite for synthesize-squad-review.py.

Spec: canon/squad-synthesizer-protocol.md
"""
from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

import yaml

# Load the hyphenated script by path (can't import via sys.path due to hyphens)
import importlib.util

_script = Path(__file__).parent.parent / "synthesize-squad-review.py"
_spec = importlib.util.spec_from_file_location("synthesize_squad_review", _script)
_mod = importlib.util.module_from_spec(_spec)  # type: ignore
_spec.loader.exec_module(_mod)  # type: ignore

detect_disputes = _mod.detect_disputes
gaps_tier = _mod.gaps_tier
merge_ldb = _mod.merge_ldb
synthesize = _mod.synthesize
validate_block = _mod.validate_block

PHASE = "phase-2"
BUILD = "test-build-alpha"


def _make_gate_block(gate: str, scored_by: str, verdict: str, anchors: list | None = None) -> dict:
    return {
        "gate_evaluation": {
            "schema_version": "v1.0",
            "gate": gate,
            "scored_by": scored_by,
            "scored_at": "2026-05-05T10:00:00Z",
            "build_id": BUILD,
            "phase_id": PHASE,
            "overall_verdict": verdict,
            "scored_anchors": anchors or [],
            "consulted_anchors": [],
            "ldb_components": [],
        }
    }


def _write_gate_file(directory: Path, gate: str, content: dict) -> None:
    tag = gate.strip("[]")
    (directory / f"{PHASE}-{tag}.yaml").write_text(yaml.dump(content))


class TestValidateBlock(unittest.TestCase):

    def test_valid_block_passes(self):
        block = _make_gate_block("[S]", "lightarchitects:security", "PASS")
        self.assertEqual(validate_block(block), [])

    def test_wrong_schema_version(self):
        block = _make_gate_block("[S]", "lightarchitects:security", "PASS")
        block["gate_evaluation"]["schema_version"] = "v0.9"
        errs = validate_block(block)
        self.assertTrue(any("schema_version" in e for e in errs))

    def test_missing_overall_verdict(self):
        block = _make_gate_block("[S]", "lightarchitects:security", "PASS")
        del block["gate_evaluation"]["overall_verdict"]
        errs = validate_block(block)
        self.assertTrue(any("overall_verdict" in e for e in errs))

    def test_citation_violation_on_fail_without_quote(self):
        anchor = {"uuid": "abc-123", "verdict": "FAIL"}
        block = _make_gate_block("[S]", "lightarchitects:security", "FAIL", [anchor])
        errs = validate_block(block)
        self.assertTrue(any("CITATION_VIOLATION" in e for e in errs))

    def test_no_citation_violation_when_quote_present(self):
        anchor = {
            "uuid": "abc-123",
            "verdict": "FAIL",
            "verbatim_anchor_quote": "The system SHALL enforce MFA.",
        }
        block = _make_gate_block("[S]", "lightarchitects:security", "FAIL", [anchor])
        errs = validate_block(block)
        self.assertFalse(any("CITATION_VIOLATION" in e for e in errs))

    def test_invalid_gate(self):
        block = _make_gate_block("[Z]", "lightarchitects:security", "PASS")
        errs = validate_block(block)
        self.assertTrue(any("[Z]" in e for e in errs))


class TestGapsTier(unittest.TestCase):

    def test_zero_gaps_is_pass(self):
        self.assertEqual(gaps_tier(0, False), "PASS")

    def test_one_gap_is_gaps_noted(self):
        self.assertEqual(gaps_tier(1, False), "GAPS_NOTED")

    def test_two_gaps_is_gaps_noted(self):
        self.assertEqual(gaps_tier(2, False), "GAPS_NOTED")

    def test_three_gaps_is_fixes_required(self):
        self.assertEqual(gaps_tier(3, False), "FIXES_REQUIRED")

    def test_any_fail_is_fixes_required(self):
        self.assertEqual(gaps_tier(0, True), "FIXES_REQUIRED")

    def test_fail_overrides_low_count(self):
        self.assertEqual(gaps_tier(1, True), "FIXES_REQUIRED")


class TestMergeLdb(unittest.TestCase):

    def test_single_block(self):
        block = _make_gate_block("[S]", "lightarchitects:security", "PASS")
        block["gate_evaluation"]["ldb_components"] = [
            {"id": "D6a", "interval": {"low": 80, "point": 85, "high": 90}}
        ]
        result = merge_ldb([block])
        self.assertEqual(result["D6a"]["point"], 85)

    def test_two_blocks_same_component_aggregated(self):
        b1 = _make_gate_block("[S]", "lightarchitects:security", "PASS")
        b1["gate_evaluation"]["ldb_components"] = [
            {"id": "D6a", "interval": {"low": 80, "point": 85, "high": 90}}
        ]
        b2 = _make_gate_block("[Q]", "lightarchitects:quality", "PASS")
        b2["gate_evaluation"]["ldb_components"] = [
            {"id": "D6a", "interval": {"low": 70, "point": 75, "high": 80}}
        ]
        result = merge_ldb([b1, b2])
        self.assertEqual(result["D6a"]["low"], 70)
        self.assertEqual(result["D6a"]["point"], 80)  # mean(85, 75)
        self.assertEqual(result["D6a"]["high"], 90)


class TestDetectDisputes(unittest.TestCase):

    def test_no_disputes_when_consistent(self):
        anchor = {"uuid": "uuid-1", "verdict": "PASS"}
        b1 = _make_gate_block("[S]", "lightarchitects:security", "PASS", [anchor])
        b2 = _make_gate_block("[Q]", "lightarchitects:quality", "PASS", [anchor])
        disputes = detect_disputes([b1, b2])
        self.assertEqual(disputes, [])

    def test_dispute_detected_when_verdicts_differ(self):
        a_pass = {"uuid": "uuid-1", "verdict": "PASS"}
        a_gaps = {"uuid": "uuid-1", "verdict": "GAPS"}
        b1 = _make_gate_block("[S]", "lightarchitects:security", "PASS", [a_pass])
        b2 = _make_gate_block("[Q]", "lightarchitects:quality", "GAPS", [a_gaps])
        disputes = detect_disputes([b1, b2])
        self.assertEqual(len(disputes), 1)
        self.assertEqual(disputes[0]["anchor_uuid"], "uuid-1")


class TestSynthesize(unittest.TestCase):

    def _setup_build(self, gate_blocks: dict[str, dict]) -> Path:
        tmpdir = Path(tempfile.mkdtemp())
        gate_dir = tmpdir / ".gate-evals"
        gate_dir.mkdir()
        for gate, content in gate_blocks.items():
            _write_gate_file(gate_dir, gate, content)
        return tmpdir

    def _read_review(self, build_root: Path) -> dict:
        return yaml.safe_load((build_root / ".squad" / "squad-review.yaml").read_text())

    def test_all_9_gates_pass_yields_pass(self):
        gate_blocks = {
            "[A]": _make_gate_block("[A]", "lightarchitects:engineer", "PASS"),
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "PASS"),
            "[Q]": _make_gate_block("[Q]", "lightarchitects:quality", "PASS"),
            "[C]": _make_gate_block("[C]", "lightarchitects:quality", "PASS"),
            "[P]": _make_gate_block("[P]", "lightarchitects:ops", "PASS"),
            "[T]": _make_gate_block("[T]", "lightarchitects:testing", "PASS"),
            "[D]": _make_gate_block("[D]", "lightarchitects:knowledge", "PASS"),
            "[O]": _make_gate_block("[O]", "lightarchitects:ops", "PASS"),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "PASS"),
            "[R]": _make_gate_block("[R]", "lightarchitects:researcher", "PASS"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 0)
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "PASS")
        self.assertIsNone(review["squad_review"]["veto_applied"])

    def test_security_fail_yields_fail_via_veto(self):
        gate_blocks = {
            "[A]": _make_gate_block("[A]", "lightarchitects:engineer", "PASS"),
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "FAIL"),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "PASS"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 2)
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "FAIL")
        # veto_applied is now a list
        self.assertIsNotNone(review["squad_review"]["veto_applied"])
        veto_gates = [v["gate"] for v in review["squad_review"]["veto_applied"]]
        self.assertIn("[S]", veto_gates)

    def test_knowledge_fail_yields_fail_via_veto(self):
        gate_blocks = {
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "PASS"),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "FAIL"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 2)
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "FAIL")

    def test_canon_gate_fail_yields_fail_via_veto(self):
        gate_blocks = {
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "PASS"),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "PASS"),
            "[C]": _make_gate_block("[C]", "lightarchitects:quality", "FAIL"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 2)
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "FAIL")
        veto_gates = [v["gate"] for v in review["squad_review"]["veto_applied"]]
        self.assertIn("[C]", veto_gates)

    def test_research_gate_fail_yields_fail_no_veto(self):
        """[R] is blocking but NOT a veto authority — fails via overall_verdict aggregation."""
        gate_blocks = {
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "PASS"),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "PASS"),
            "[R]": _make_gate_block("[R]", "lightarchitects:researcher", "FAIL"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 1)  # FIXES_REQUIRED (overall_verdict FAIL on non-veto gate)
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "FIXES_REQUIRED")
        self.assertIsNone(review["squad_review"]["veto_applied"])

    def test_missing_researcher_warns_but_doesnt_fail(self):
        """[R] absence produces a warning but not a FAIL (non-quorum)."""
        gate_blocks = {
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "PASS"),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "PASS"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 0)
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "PASS")
        warnings = review["squad_review"].get("missing_warnings", [])
        self.assertTrue(any("[R]" in w for w in warnings))

    def test_two_gaps_yields_gaps_noted(self):
        gap1 = {"uuid": "u1", "verdict": "GAPS", "verbatim_anchor_quote": "q"}
        gap2 = {"uuid": "u2", "verdict": "GAPS", "verbatim_anchor_quote": "q"}
        gate_blocks = {
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "GAPS", [gap1]),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "GAPS", [gap2]),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 0)  # GAPS_NOTED still exits 0
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "GAPS_NOTED")

    def test_three_gaps_yields_fixes_required(self):
        def gap(uid: str) -> dict:
            return {"uuid": uid, "verdict": "GAPS", "verbatim_anchor_quote": "q"}

        gate_blocks = {
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "GAPS", [gap("u1"), gap("u2")]),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "GAPS", [gap("u3")]),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 1)  # FIXES_REQUIRED exits 1
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "FIXES_REQUIRED")

    def test_missing_security_quorum_fails(self):
        gate_blocks = {
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "PASS"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 2)
        review = self._read_review(root)
        self.assertEqual(review["squad_review"]["verdict"], "FAIL")

    def test_validate_only_does_not_write_output(self):
        gate_blocks = {
            "[S]": _make_gate_block("[S]", "lightarchitects:security", "PASS"),
            "[K]": _make_gate_block("[K]", "lightarchitects:knowledge", "PASS"),
        }
        root = self._setup_build(gate_blocks)
        rc = synthesize(root, PHASE, validate_only=True)
        self.assertEqual(rc, 0)
        self.assertFalse((root / ".squad" / "squad-review.yaml").exists())

    def test_no_gate_blocks_exits_3(self):
        root = Path(tempfile.mkdtemp())
        (root / ".gate-evals").mkdir()
        rc = synthesize(root, PHASE)
        self.assertEqual(rc, 3)


if __name__ == "__main__":
    unittest.main(verbosity=2)
