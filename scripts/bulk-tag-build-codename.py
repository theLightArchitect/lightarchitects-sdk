#!/usr/bin/env python3
"""H.9 — bulk-tag queue tasks with build_codename.

Reads ~/.lightarchitects/tasks/queue.json, applies build_codename annotations
to tasks that match the convergent-shipping-armada constituent builds, and
writes the result back atomically (tmp → rename).

Usage:
    python3 scripts/bulk-tag-build-codename.py [--dry-run]

Flags:
    --dry-run   Print the proposed changes without writing queue.json.

Mapping rules are evaluated in order; first match wins. Tasks already tagged
with a build_codename are skipped (idempotent).
"""

import json
import os
import sys
import re
import shutil
from pathlib import Path

# ── Mapping rules ──────────────────────────────────────────────────────────────
# Each rule is (build_codename, list_of_regex_patterns_matching_title).
# First matching rule wins. Case-insensitive match against task title.

RULES = [
    # vault-migration-v1-ph3
    ("vault-migration-v1-ph3", [
        r"vault.*migration",
        r"migration.*vault",
        r"neo4j.*migration",
        r"vault.*phase.?3",
    ]),

    # embodied-engineering-forge
    ("embodied-engineering-forge", [
        r"eef",
        r"embodied.engineering",
        r"agentrunner",
        r"vibe.cod",
    ]),

    # squad-comms-operator-ui (UI components + operator surfaces)
    ("squad-comms-operator-ui", [
        r"squad comms.*ui",
        r"squad comms.*svelte",
        r"squad comms.*webshell",
        r"squad comms.*per.project.*route",
        r"squad comms.*full.scope.*re.impl",
        r"squad comms.*re.impl",
        r"comms.*operator",
        r"comms.*view.mode",
        r"commsdashboard",
        r"spawn.worker",
        r"audit the security surface",
        r"audit security surface",
    ]),

    # squad-comms-task-ingest (gateway/CLI/backend/security/deploy)
    ("squad-comms-task-ingest", [
        r"squad comms.*gateway",
        r"squad comms.*mcp",
        r"squad comms.*cli",
        r"squad comms.*subcommand",
        r"squad comms.*security",
        r"squad comms.*guard",
        r"squad comms.*deploy",
        r"sqd.foxtrot",
        r"^deploy service$",
        r"sqd.echo",
        r"sqd.f8",
        r"fanout all eight",
        r"squad.*pin worktree",
        r"worktree.*base.ref",
        r"base_ref parameter",
    ]),

    # squishy-dancing-thimble / webshell-ayin-traces
    ("webshell-ayin-traces", [
        r"ayin.*trac",
        r"squad comms.*ayin",
        r"ayin.*span",
        r"sqd.delta",
        r"squishy",
        r"webshell.*ayin",
    ]),

    # functional-forging-queue / webshell-build-ux
    ("webshell-build-ux", [
        r"build.ux",
        r"functional.forging",
        r"build.*queue.*ux",
        r"ux.*build.*queue",
    ]),

    # webshell-testing-mvp
    ("webshell-testing-mvp", [
        r"webshell.*test",
        r"test.*webshell",
        r"setup.flow",
        r"setupflow",
        r"phase a:.*webshell",
        r"playwright.*webshell",
        r"e2e.*webshell",
        r"\-\-user.*integration",
    ]),

    # webshell-copilot-mvp (eva-copilot-voice)
    ("webshell-copilot-mvp", [
        r"eva.*persona",
        r"copilot.*voice",
        r"eva.*voice",
        r"copilot.*persona",
        r"phase b:.*eva",
        r"pair.programmer.*voice",
    ]),

    # soul-4signal-rrf
    ("soul-4signal-rrf", [
        r"soul.*cross.process.*inject",
        r"cross.process.*chat",
        r"file.backed.*inbox",
        r"4.signal.*rrf",
        r"rrf.*soul",
        r"soul.*rrf",
        r"soul.*agentic.*re.query",
        r"agentic.*re.query",
    ]),
]


def match_rule(title: str) -> str | None:
    t = title.lower()
    for codename, patterns in RULES:
        for pat in patterns:
            if re.search(pat, t):
                return codename
    return None


def main() -> None:
    dry_run = "--dry-run" in sys.argv

    queue_path = Path.home() / ".lightarchitects" / "tasks" / "queue.json"
    if not queue_path.exists():
        print(f"ERROR: queue not found at {queue_path}", file=sys.stderr)
        sys.exit(1)

    with queue_path.open() as fh:
        data = json.load(fh)

    tasks = data.get("tasks", [])
    tagged = 0
    skipped = 0
    unmatched = []

    for task in tasks:
        if task.get("build_codename"):
            skipped += 1
            continue
        codename = match_rule(task.get("title", ""))
        if codename:
            if dry_run:
                print(f"  [DRY] {task['id']:35} -> {codename}  | {task.get('title','')[:60]}")
            else:
                task["build_codename"] = codename
            tagged += 1
        else:
            unmatched.append(task)

    print(f"\nResults: {tagged} tagged, {skipped} already-tagged skipped, "
          f"{len(unmatched)} unmatched")

    if unmatched:
        print("\nUnmatched tasks (no rule applied):")
        for t in unmatched:
            if t.get("status") in ("pending", "in_progress"):
                print(f"  [{t['status']:12}] {t['id']:35} | {t.get('title','')[:60]}")

    if dry_run:
        print("\n[dry-run] No changes written.")
        return

    tmp = queue_path.with_suffix(".tmp")
    with tmp.open("w") as fh:
        json.dump(data, fh, indent=2)
    shutil.move(str(tmp), str(queue_path))
    print(f"\nWrote {queue_path}")


if __name__ == "__main__":
    main()
