#!/usr/bin/env bash
# Validate all contract YAMLs in standards/canon/contracts/ against la-contracts.schema.json
# Then sweep cross-contract symmetric-edge invariants (LÆX ratification 2026-06-03 §7.1 Q3):
#   - every wire.mcp.hosted_by_mcp_capability_contract_id must reference an existing mcp.capability
#   - every mcp.capability.exposed_wire_mcp_contract_ids entry must reference an existing wire.mcp
#   - the pair must be reciprocal: a points at b AND b points at a (no half-edges)
# Exit 0 if all pass, 1 if any fail.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCHEMA="$(cd "$SCRIPT_DIR/.." && pwd)/la-contracts.schema.json"

if [[ ! -f "$SCHEMA" ]]; then
    echo "ERROR: schema not found at $SCHEMA" >&2
    exit 2
fi

python3 - "$SCHEMA" "$SCRIPT_DIR" <<'PY'
import json, sys, yaml, pathlib
from collections import Counter, defaultdict
try:
    from jsonschema import Draft202012Validator
except ImportError:
    print("ERROR: pip install jsonschema (or pip3 install jsonschema)", file=sys.stderr)
    sys.exit(2)

schema_path, contracts_root = sys.argv[1], sys.argv[2]
schema = json.load(open(schema_path))
Draft202012Validator.check_schema(schema)
v = Draft202012Validator(schema)

stubs = sorted(pathlib.Path(contracts_root).rglob('*.yaml'))
if not stubs:
    print("No contract YAMLs found.", file=sys.stderr)
    sys.exit(2)

# ── Pass 1: per-file schema validation ──────────────────────────────────────
ok = 0
errs = []
class_counts = Counter()
instances = {}  # contract_id → instance (for symmetric-edge pass)
for f in stubs:
    try:
        inst = yaml.safe_load(f.read_text())
    except yaml.YAMLError as e:
        errs.append((f, [f"YAML parse error: {e}"]))
        class_counts['yaml_parse_error'] += 1
        continue
    file_errs = list(v.iter_errors(inst))
    if not file_errs:
        ok += 1
        if isinstance(inst, dict) and 'id' in inst:
            instances[inst['id']] = (f, inst)
    else:
        e = file_errs[0]
        msg = e.message
        cls = 'unknown'
        if 'required property' in msg: cls = 'missing_required'
        elif 'does not match' in msg: cls = 'pattern_mismatch'
        elif 'is not one of' in msg: cls = 'enum_violation'
        elif 'is too' in msg: cls = 'length_violation'
        class_counts[cls] += 1
        errs.append((f, [f"{' / '.join(str(p) for p in e.absolute_path) or '<root>'}: {e.message[:200]}" for e in file_errs[:3]]))

total = len(stubs)
print(f"\n{ok}/{total} contracts validate ({ok * 100 // total}%)")
if errs:
    print(f"\n{len(errs)} schema failures by class:")
    for c, n in class_counts.most_common():
        print(f"  {n:3} × {c}")
    print()
    for f, file_errs in errs[:20]:
        print(f"FAIL {f.relative_to(pathlib.Path(contracts_root).parent.parent)}")
        for em in file_errs:
            print(f"  → {em}")
    if len(errs) > 20:
        print(f"  ({len(errs) - 20} more failures suppressed)")
    sys.exit(1)

# ── Pass 2: symmetric-edge sweep (LÆX 2026-06-03 §7.1 Q3) ───────────────────
# Build directed-edge sets:
#   forward[cap_id] = set of wire.mcp contract ids the capability advertises
#   backward[wire_id] = the single mcp.capability contract id the wire claims to be hosted by
forward = defaultdict(set)
backward = {}
mcp_capability_ids = set()
wire_mcp_ids = set()

for cid, (f, inst) in instances.items():
    kind = inst.get('kind', '')
    if kind == 'mcp.capability':
        mcp_capability_ids.add(cid)
        exposed = inst.get('mcp_capability', {}).get('exposed_wire_mcp_contract_ids', [])
        for wid in exposed:
            forward[cid].add(wid)
    elif kind == 'wire.mcp':
        wire_mcp_ids.add(cid)
        host = inst.get('wire_mcp', {}).get('hosted_by_mcp_capability_contract_id')
        if host is not None:
            backward[cid] = host

# Check every forward edge resolves AND is reciprocated
edge_violations = []
for cap_id, wire_targets in forward.items():
    for wire_id in wire_targets:
        if wire_id not in instances:
            edge_violations.append({
                'kind': 'dangling_forward',
                'from': cap_id,
                'to': wire_id,
                'detail': f"mcp.capability '{cap_id}' lists wire.mcp '{wire_id}' but no such contract exists on disk"
            })
            continue
        if instances[wire_id][1].get('kind') != 'wire.mcp':
            edge_violations.append({
                'kind': 'wrong_kind_forward',
                'from': cap_id,
                'to': wire_id,
                'detail': f"mcp.capability '{cap_id}' lists '{wire_id}' but that contract has kind={instances[wire_id][1].get('kind')}, not wire.mcp"
            })
            continue
        if backward.get(wire_id) != cap_id:
            actual = backward.get(wire_id, '<missing>')
            edge_violations.append({
                'kind': 'unreciprocated_forward',
                'from': cap_id,
                'to': wire_id,
                'detail': f"mcp.capability '{cap_id}' lists wire.mcp '{wire_id}', but that wire.mcp's hosted_by_mcp_capability_contract_id = '{actual}'"
            })

# Check every backward edge resolves AND is reciprocated
for wire_id, cap_id in backward.items():
    if cap_id not in instances:
        edge_violations.append({
            'kind': 'dangling_backward',
            'from': wire_id,
            'to': cap_id,
            'detail': f"wire.mcp '{wire_id}' is hosted_by '{cap_id}' but no such contract exists on disk"
        })
        continue
    if instances[cap_id][1].get('kind') != 'mcp.capability':
        edge_violations.append({
            'kind': 'wrong_kind_backward',
            'from': wire_id,
            'to': cap_id,
            'detail': f"wire.mcp '{wire_id}' is hosted_by '{cap_id}' but that contract has kind={instances[cap_id][1].get('kind')}, not mcp.capability"
        })
        continue
    if wire_id not in forward.get(cap_id, set()):
        edge_violations.append({
            'kind': 'unreciprocated_backward',
            'from': wire_id,
            'to': cap_id,
            'detail': f"wire.mcp '{wire_id}' is hosted_by mcp.capability '{cap_id}', but that capability's exposed_wire_mcp_contract_ids does NOT list '{wire_id}'"
        })

# Report
mcp_pairs = sum(1 for cid in forward for _ in forward[cid])
print(f"\nSymmetric-edge sweep (mcp.capability ↔ wire.mcp, LÆX 2026-06-03):")
print(f"  mcp.capability contracts:    {len(mcp_capability_ids)}")
print(f"  wire.mcp contracts:          {len(wire_mcp_ids)}")
print(f"  forward edges declared:      {mcp_pairs}")
print(f"  backward edges declared:     {len(backward)}")

if edge_violations:
    by_kind = Counter(e['kind'] for e in edge_violations)
    print(f"\n{len(edge_violations)} symmetric-edge violations:")
    for k, n in by_kind.most_common():
        print(f"  {n:3} × {k}")
    print()
    for ev in edge_violations[:20]:
        print(f"FAIL [{ev['kind']}] {ev['from']} ↛ {ev['to']}")
        print(f"  → {ev['detail']}")
    if len(edge_violations) > 20:
        print(f"  ({len(edge_violations) - 20} more violations suppressed)")
    sys.exit(1)

print(f"  ✓ all edges reciprocated\n")
sys.exit(0)
PY
