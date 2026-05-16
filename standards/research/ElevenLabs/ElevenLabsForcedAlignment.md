---
id: "55093861-3d5ad3c3"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# ElevenLabs — Forced Alignment API

> Researched: 2026-02-28 | Source: elevenlabs-python SDK (GitHub), official docs

---

## Overview

Forced alignment maps an **existing audio file** to a **text transcript** — returning character-level
and word-level timestamps with confidence scores. It does **not generate audio**.

**Use case**: Add subtitle/caption sync to pre-existing recordings; validate timing of voice actor
takes; analyze prosody of recorded dialogue.

---

## Endpoint

```
POST /v1/forced-alignment
Content-Type: multipart/form-data
```

---

## Request Parameters

| Parameter | Type | Required | Notes |
|-----------|------|----------|-------|
| `audio` | `file` (multipart) | Yes | Audio file to align. Use `enabled_spooled_file` for large files (> 5MB). |
| `transcript` | `str` | Yes | Text to align against. Must match the spoken content. |

### `enabled_spooled_file`

For audio files larger than 5MB, use the SDK's spooled file wrapper to avoid loading the
entire file into memory before the upload begins:

```python
from elevenlabs.utils import enabled_spooled_file

with enabled_spooled_file(open("long_recording.mp3", "rb"), max_size=5_000_000) as f:
    response = client.audio.forced_alignment(
        audio=f,
        transcript="...",
    )
```

---

## Response: `ForcedAlignmentResponseModel`

```python
class ForcedAlignmentResponseModel:
    characters:          List[ForcedAlignmentCharacterResponseModel]
    words:               List[ForcedAlignmentWordResponseModel]
    quality_score:       float   # deprecated — not present in current responses
    loss:                float   # primary quality metric (lower = better alignment)
```

### `ForcedAlignmentCharacterResponseModel`

```python
class ForcedAlignmentCharacterResponseModel:
    character:           str
    start_time_seconds:  float
    end_time_seconds:    float
```

### `ForcedAlignmentWordResponseModel`

```python
class ForcedAlignmentWordResponseModel:
    word:                str
    start_time_seconds:  float
    end_time_seconds:    float
```

---

## Quality Metric: `loss`

The `loss` field is the primary quality indicator.

| `loss` range | Interpretation |
|-------------|----------------|
| < 0.1 | Excellent alignment — high confidence |
| 0.1 – 0.3 | Good alignment — usable for subtitles |
| 0.3 – 0.6 | Moderate — review manually at boundaries |
| > 0.6 | Poor — mismatch between audio and transcript |

**When to reject**: If `loss > 0.5`, the transcript likely does not match the audio. Check for:
- Omitted words or pauses in the recording
- Heavy accent variance from the transcript
- Background noise or music affecting alignment

---

## Python SDK Example

```python
from elevenlabs import ElevenLabs

client = ElevenLabs(api_key="YOUR_API_KEY")

with open("seraph_monologue.mp3", "rb") as f:
    response = client.audio.forced_alignment(
        audio=f,
        transcript="I have been watching this network for six days. Three beacons. Two exposed ports.",
    )

print(f"Alignment loss: {response.loss:.4f}")

# Character-level timestamps
for char in response.characters:
    print(f"  '{char.character}': {char.start_time_seconds:.3f}s – {char.end_time_seconds:.3f}s")

# Word-level timestamps
for word in response.words:
    print(f"  [{word.word}]: {word.start_time_seconds:.3f}s – {word.end_time_seconds:.3f}s")
```

---

## Large File Example (spooled)

```python
from elevenlabs import ElevenLabs
from elevenlabs.utils import enabled_spooled_file

client = ElevenLabs(api_key="YOUR_API_KEY")

# 5MB threshold — files larger than this stream without full memory load
with enabled_spooled_file(open("squad_conversation_long.mp3", "rb"), max_size=5_000_000) as f:
    response = client.audio.forced_alignment(
        audio=f,
        transcript=long_transcript,
    )

if response.loss > 0.5:
    raise ValueError(f"Alignment failed (loss={response.loss:.3f}) — check transcript match")
```

---

## Key Limitations

- Does **not** generate audio — maps existing recordings only
- Transcript must match spoken content closely (gaps cause high `loss`)
- `quality_score` field is **deprecated** — use `loss` exclusively
- No speaker diarization — if multiple speakers are present, the timestamp stream is continuous
- No confidence per-character — only a single `loss` for the full alignment

---

## Squad Usage Pattern

Forced alignment is useful for validating that a TTS render matches the intended transcript
before committing to a squad voice pipeline:

```python
# Step 1: Generate audio via SOUL speak (or text-to-dialogue)
# Step 2: Align the returned audio against the script

with open("generated_seraph.mp3", "rb") as f:
    alignment = client.audio.forced_alignment(
        audio=f,
        transcript=seraph_script,
    )

if alignment.loss > 0.3:
    print(f"WARNING: TTS alignment loss high ({alignment.loss:.3f}) — review output")

# Step 3: Export word-level timestamps for subtitle sync
subtitles = [
    {"word": w.word, "start": w.start_time_seconds, "end": w.end_time_seconds}
    for w in alignment.words
]
```

This pattern enables quality-gated TTS: reject renders where the model dropped words or
slurred phonemes that the alignment algorithm can detect.

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
