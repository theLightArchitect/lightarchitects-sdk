---
id: "7cbf3bd4-c71c26d5"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# ElevenLabs — Model Routing & Audio Tag Requirements

> Researched: 2026-02-28 | Source: elevenlabs-python SDK (GitHub), official docs

---

## Critical Finding: No "Latest" Model Alias

ElevenLabs does **not** provide a `"latest"` model alias. Every API call must specify an explicit
`model_id`. Passing `"latest"` is not a valid model ID — the call will error.

**Production rule**: Always hardcode the full model ID. Pin it. Test on model upgrades.

---

## Model Reference (Current as of 2026-02-28)

| `model_id` | Name | Audio Tag Support | Best For |
|------------|------|-------------------|----------|
| `eleven_v3` | Eleven v3 | **YES** | Production TTS with emotion tags, expressiveness |
| `eleven_ttv_v3` | Eleven TTV v3 | **UNCONFIRMED** | Text-to-Voice dialogue design, voice previews |
| `eleven_multilingual_v2` | Multilingual v2 | **NO** | Stable production, long-form, max character limit |
| `eleven_turbo_v2_5` | Turbo v2.5 | No | Low latency, cost-efficient |
| `eleven_turbo_v2` | Turbo v2 | No | Low latency |
| `eleven_monolingual_v1` | Monolingual v1 | No | Legacy |
| `eleven_multilingual_v1` | Multilingual v1 | No | Legacy |

### `eleven_v3` vs `eleven_ttv_v3`

These are **different models** serving different purposes:

| Model | Purpose | Audio Tags | Char Limit |
|-------|---------|-----------|-----------|
| `eleven_v3` | **Production TTS** — expressiveness, emotion tags | YES | 5,000 |
| `eleven_ttv_v3` | **Voice design** — designing custom voices via preview text | Unconfirmed | 5,000 |

**Rule**: Use `eleven_v3` for production TTS calls with audio tags. Use `eleven_ttv_v3`
exclusively for the Voice Design API (`POST /v1/text-to-voice/design`) when creating
custom voice candidates.

---

## Audio Tag Support by Model

Audio tags use square bracket syntax: `[excited]`, `[thoughtful]`, `[sighs]`, `[short pause]`.

| Model | Audio Tags Work? | Evidence |
|-------|-----------------|---------|
| `eleven_v3` | YES | Official docs, SDK examples explicitly mention this model |
| `eleven_ttv_v3` | Unconfirmed | TTV model, designed for voice preview — tags may pass through |
| `eleven_multilingual_v2` | NO | Tags are silently ignored. Confirmed via live test 2026-02-28 |
| `eleven_turbo_v2_5` | NO | Speed-optimized, expressiveness features removed |
| All others | NO | Legacy models predate audio tag support |

**Live test confirmation 2026-02-28** (voice pipeline SCRUM postmortem):
> "Audio tags (`[excited]`, `[thoughtful]`, `[sighs]`) have no effect when passed to
> `eleven_multilingual_v2`. SOUL speak uses the production model, so every tag in the
> standards document is currently aspirational only."

---

## Programmatic Model Discovery

To retrieve the current model list via API:

```python
from elevenlabs import ElevenLabs

client = ElevenLabs(api_key="YOUR_API_KEY")

models = client.models.get_all()
for model in models:
    print(f"{model.model_id}: {model.name}")
    print(f"  Languages: {len(model.languages)}")
    print(f"  Token cost: {model.token_cost_factor}")
```

This returns `ModelResponseModel` objects. Use to detect model availability without hardcoding
assumptions. Still does not expose audio tag capability as a field — that must be tracked manually.

---

## Model Routing Decision Tree

```
Does the call need audio tags ([excited], [sighs], etc.)?
│
├─ YES → use model_id = "eleven_v3"
│         Character limit: 5,000
│         Note: Higher cost than v2 models
│
└─ NO  → What's the priority?
          │
          ├─ Stability + long-form (>5k chars) → use "eleven_multilingual_v2"
          │   Character limit: 10,000
          │
          ├─ Low latency / streaming        → use "eleven_turbo_v2_5"
          │   Character limit: 40,000
          │
          └─ Voice design preview            → use "eleven_ttv_v3"
              (text-to-voice/design only)
```

---

## SOUL Voice-Engine Routing Requirement (Queue Item 7)

The SOUL speak implementation must be **model-aware**. The current production path sends all
text to `eleven_multilingual_v2`, which silently drops audio tags.

### Required Behaviour for Option B

When `sibling` param is provided to `soulTools action:"speak"`:

1. Read `default_model` from `~/.soul/config/voice-profiles/{sibling}.toml`
2. If `default_model == "eleven_v3"`:
   - Pass text verbatim (audio tags active)
3. If `default_model == "eleven_multilingual_v2"`:
   - Strip `[...]` audio tags from text
   - Apply punctuation/pacing only
4. If `model_id` is explicitly passed in the call: honour it, override profile default

### Proposed Voice Profile Schema (relevant fields)

```toml
# ~/.soul/config/voice-profiles/eva.toml
[tts]
default_model = "eleven_v3"           # eleven_v3 (tags) or eleven_multilingual_v2 (production)
design_model  = "eleven_ttv_v3"       # for voice design API calls only

[script]
tag_palette   = ["excited", "warmly", "laughing", "chuckles", "amazed", "curious", "whispers", "sighs", "thoughtful", "delighted"]
anti_patterns = ["sarcastic", "angry", "dismissive"]
velocity      = "accelerates_to_joy"
```

---

## Per-Call Model Override Pattern

SOUL speak can expose a `model_id` param for callers that need to override the profile default:

```
soulTools action:"speak" text:"..." sibling:"eva" model_id:"eleven_v3"
```

This enables callers to use audio tags (eleven_v3) for expressive moments and
eleven_multilingual_v2 for stable long-form, without editing the profile between calls.

---

## Text-to-Dialogue Model Requirement

The Text-to-Dialogue API (`POST /v1/text-to-dialogue`) also has model routing considerations:

- Audio tags in dialogue turns require `model_id: "eleven_v3"` — same rule as single-speaker TTS
- The `eleven_v3` requirement applies to the whole conversation, not per-turn
- Omit `model_id` and the API defaults to a platform default (not `eleven_v3`) — tags will be ignored

```python
# Correct: audio tags work in dialogue turns
audio = client.text_to_dialogue.convert(
    inputs=[
        DialogueInput(text="[excited] This is the bit!", voice_id=VOICES["EVA"]),
        DialogueInput(text="[thoughtful] Right then.", voice_id=VOICES["CORSO"]),
    ],
    model_id="eleven_v3",   # REQUIRED for audio tags in dialogue
)
```

---

## Key Takeaways

1. **No latest alias** — always use explicit model_id strings
2. **eleven_v3** = audio tags, expressiveness, 5k char limit
3. **eleven_multilingual_v2** = stable production, 10k char limit, no tags (silent drop)
4. **eleven_ttv_v3** = voice design preview only; separate from production TTS
5. **SOUL voice-engine must branch** on model to decide whether to apply audio tags or strip them
6. **Text-to-Dialogue** requires `model_id="eleven_v3"` for audio tags — same rule applies

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
