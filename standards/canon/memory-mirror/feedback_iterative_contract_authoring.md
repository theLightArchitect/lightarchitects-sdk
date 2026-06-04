---
name: iterative-contract-authoring
description: "When bulk-authoring contracts against a JSON Schema, validate after EACH contract — not after a batch — to localize enum/shape errors"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 767e46bb-eb90-4ad0-a585-e6f528850e34
---

When authoring 5+ contracts against a JSON Schema discriminator, run the validator after each contract — not after a batch. Schema enum constraints (status, type, ttl, screen_key, evidence_tier) are not obvious from prose; the validator's error messages are the fastest path to discovering them.

**Why:** Schemas have implicit enum constraints surfaced only at validation time. Common drift caught in Wave A 2026-06-04:
- `type: bool` → must be `boolean`
- `type: list_of_enum` → must be `array` (with description for element constraint)
- `ttl: ephemeral` → must be one of `[session, forever, until-make-clean, until-operator-deletion]`
- `screen_key: Copilot` → must be one of `[Dashboard, Dispatch, Builds, Intake, Helix, ...]`
- `history_continuity_on_provider_switch: not_applicable` → must be `n/a`

Batching 20 contracts then validating means each fix may cascade through 20 files. Per-contract validation localizes each error to the file that introduced it.

**How to apply:** Loop pattern when authoring N contracts of the same kind:

```
for contract in batch:
    write contract
    run ./target/release/contract-gate --schema X --contracts-dir Y
    if fail: fix the surfaced enum/shape error before next write
    commit only after entire batch validates
```

Identify VARIABLE fields (typically 5-10: operator_intent, inputs, observable_outputs, forbidden_behaviors, conformance_test, discriminator block) vs INVARIANT fields (status_per_provider matrix, observability template, schema_version/kind/version). Copy invariant; vary variable. Saves ~30% authoring time per contract after the first one.

Pressure-tested 2026-06-04 Wave A: 24 contracts authored against `la-contracts.schema.json` v1.2, 4-5 enum violations surfaced + fixed in <2min each. Batching all 24 first would have cascaded 24×4 = 96 fixes. See related: [[la-contracts-schema-v1-2-shipped]].
