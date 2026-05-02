#!/usr/bin/env python3
"""Wave 4.5 color literal sweep — replace [#HEX] Tailwind arbitrary values with [var(--la-*)]."""
import re, os, sys
from pathlib import Path

# Canonical mapping: hex (lowercase) → CSS var
# Based on tokens.css + design-tokens.ts actual values
MAPPING: list[tuple[str, str]] = [
    # ── Backgrounds ──────────────────────────────────────────────────────────
    ("#08090a", "var(--la-bg-void)"),
    ("#0a0a0a", "var(--la-bg-void)"),
    ("#0a0a0f", "var(--la-bg-frame)"),
    ("#0a0a12", "var(--la-bg-frame)"),
    ("#0c0d0e", "var(--la-bg-frame)"),
    ("#0d0d14", "var(--la-bg-frame)"),
    ("#0d1117", "var(--la-drawer-bg)"),
    ("#0f1117", "var(--la-bg-frame)"),
    ("#0f172a", "var(--la-bg-frame)"),
    ("#111214", "var(--la-bg-elev-1)"),
    ("#111827", "var(--la-bg-elev-1)"),
    ("#16181b", "var(--la-bg-elev-2)"),
    # ── Hairlines / borders ───────────────────────────────────────────────────
    ("#1e293b", "var(--la-drawer-border)"),
    ("#25282d", "var(--la-hair-base)"),
    ("#334155", "var(--la-hair-strong)"),
    ("#3a3f47", "var(--la-hair-strong)"),
    # ── Text scale ────────────────────────────────────────────────────────────
    ("#3e434a", "var(--la-text-mute)"),
    ("#475569", "var(--la-text-dim)"),
    ("#4b5563", "var(--la-text-dim)"),
    ("#5d646e", "var(--la-text-dim)"),
    ("#64748b", "var(--la-text-dim)"),
    ("#6b7280", "var(--la-text-base)"),
    ("#8a929c", "var(--la-text-base)"),
    ("#94a3b8", "var(--la-text-label)"),
    ("#9ca3af", "var(--la-text-label)"),
    ("#a3acb9", "var(--la-text-label)"),
    ("#cbd5e1", "var(--la-text-bright)"),
    ("#d1d5db", "var(--la-text-bright)"),
    ("#d8dde4", "var(--la-text-bright)"),
    ("#e2e8f0", "var(--la-text-bright)"),
    ("#f1f5f9", "var(--la-text-stark)"),
    ("#f6f7f8", "var(--la-text-stark)"),
    ("#fff",    "var(--la-text-stark)"),
    ("#ffffff", "var(--la-text-stark)"),
    # ── Accent / focus ring ───────────────────────────────────────────────────
    ("#d4a017", "var(--la-focus-ring)"),
    ("#ffd700", "var(--la-focus-ring)"),
    ("#ffeaa7", "var(--la-focus-ring)"),
    # ── Danger ───────────────────────────────────────────────────────────────
    ("#b91c1c", "var(--la-danger-stroke)"),
    ("#dc2626", "var(--la-danger-stroke)"),
    ("#ef4444", "var(--la-danger-stroke)"),
    ("#ff4d6a", "var(--la-danger-stroke)"),
    ("#f87171", "var(--la-danger-text)"),
    ("#fca5a5", "var(--la-danger-text)"),
    # ── Domain agent colors ───────────────────────────────────────────────────
    ("#4d8eff", "var(--la-agent-engineer)"),
    ("#3b82f6", "var(--la-agent-engineer)"),
    ("#60a5fa", "var(--la-agent-engineer)"),
    ("#f5d440", "var(--la-agent-quality)"),
    ("#f0c040", "var(--la-agent-quality)"),
    ("#fbbf24", "var(--la-agent-quality)"),
    ("#ff4d4d", "var(--la-agent-security)"),
    ("#d24df5", "var(--la-agent-ops)"),
    ("#b44aff", "var(--la-agent-ops)"),
    ("#4dff8e", "var(--la-agent-researcher)"),
    ("#22c55e", "var(--la-agent-researcher)"),
    ("#86efac", "var(--la-agent-researcher)"),
    ("#4ade80", "var(--la-agent-researcher)"),
    ("#10b981", "var(--la-agent-researcher)"),
    ("#4dffe6", "var(--la-agent-knowledge)"),
    ("#00bfff", "var(--la-agent-knowledge)"),
    ("#06b6d4", "var(--la-agent-knowledge)"),
    ("#2563eb", "var(--la-agent-engineer)"),
    ("#ff8e3c", "var(--la-agent-performance)"),
    ("#ff6d00", "var(--la-agent-performance)"),
    ("#f59e0b", "var(--la-agent-performance)"),
    ("#fb923c", "var(--la-agent-performance)"),
    ("#9f67ff", "var(--la-agent-testing)"),
    ("#a874ff", "var(--la-agent-testing)"),
    ("#8b5cf6", "var(--la-agent-testing)"),
    ("#818cf8", "var(--la-agent-testing)"),
    ("#a78bfa", "var(--la-agent-testing)"),
    ("#c4b5fd", "var(--la-agent-testing)"),
    ("#4f46e5", "var(--la-agent-testing)"),
    ("#6366f1", "var(--la-agent-testing)"),
    ("#a855f7", "var(--la-agent-testing)"),
    ("#3b0764", "var(--la-agent-testing)"),
    ("#701a75", "var(--la-agent-ops)"),
    ("#ff7eb6", "var(--la-agent-documentation)"),
    ("#ff6b9d", "var(--la-agent-documentation)"),
    ("#f9a8d4", "var(--la-agent-documentation)"),
    ("#ec4899", "var(--la-agent-documentation)"),
]

# Build regex: match [#HEX] where HEX is 3, 6, or 8 hex digits (case-insensitive)
# We replace only the #HEX part inside the brackets, preserving the brackets and
# any Tailwind opacity modifier that follows (e.g., /10).
HEX_PATTERN = re.compile(r'\[#([0-9a-fA-F]{3,8})\]', re.IGNORECASE)

# Build a fast lookup: lowercase hex → var string
lookup: dict[str, str] = {hex_val.lstrip('#'): var_val for hex_val, var_val in MAPPING}

def replace_hex(m: re.Match) -> str:
    hex_digits = m.group(1).lower()
    if hex_digits in lookup:
        return f'[{lookup[hex_digits]}]'
    return m.group(0)  # leave unmapped literals as-is (they go in the allowlist)

def sweep_file(path: Path) -> tuple[int, int]:
    """Return (original_count, remaining_count)."""
    text = path.read_text(encoding='utf-8')
    original_count = len(HEX_PATTERN.findall(text))
    if original_count == 0:
        return 0, 0
    new_text = HEX_PATTERN.sub(replace_hex, text)
    remaining = len(HEX_PATTERN.findall(new_text))
    if new_text != text:
        path.write_text(new_text, encoding='utf-8')
    return original_count, remaining

def main() -> None:
    root = Path(__file__).parent.parent / 'src'
    targets = [
        root / 'screens',
        root / 'components',
    ]

    total_original = 0
    total_remaining = 0
    unmapped: dict[str, list[str]] = {}  # hex → list of file paths

    for target_dir in targets:
        for svelte_file in sorted(target_dir.rglob('*.svelte')):
            orig, remain = sweep_file(svelte_file)
            total_original += orig
            total_remaining += remain
            if remain > 0:
                # Collect unmapped to report
                text = svelte_file.read_text(encoding='utf-8')
                for m in HEX_PATTERN.finditer(text):
                    h = m.group(1).lower()
                    rel = str(svelte_file.relative_to(root.parent))
                    unmapped.setdefault(h, []).append(rel)

    print(f"Swept: {total_original - total_remaining} / {total_original} literals replaced")
    print(f"Remaining: {total_remaining}")
    if unmapped:
        print("\nUnmapped hex values (add to MAPPING or allowlist):")
        for h, files in sorted(unmapped.items(), key=lambda x: -len(x[1])):
            print(f"  #{h}  ({len(files)} occurrences) — e.g. {files[0]}")
    else:
        print("All literals mapped. Gate: PASS")
    sys.exit(0 if total_remaining == 0 else 1)

if __name__ == '__main__':
    main()
