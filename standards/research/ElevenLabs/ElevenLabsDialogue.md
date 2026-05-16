---
id: "32f96605-8a1c5453"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# ElevenLabs — Text-to-Dialogue & Stream APIs

> Researched: 2026-02-28 | Source: elevenlabs-python SDK (GitHub), official docs

---

## Overview

Four endpoints for multi-speaker dialogue generation. All share the same
request parameters — the only differences are buffering behaviour and WAV
output support.

| Endpoint | URL | Return |
|----------|-----|--------|
| `convert()` | POST `/v1/text-to-dialogue` | `Iterator[bytes]` — buffered |
| `stream()` | POST `/v1/text-to-dialogue/stream` | `Iterator[bytes]` — chunked |
| `convert_with_timestamps()` | POST `/v1/text-to-dialogue/with-timestamps` | `AudioWithTimestampsAndVoiceSegmentsResponseModel` |
| `stream_with_timestamps()` | POST `/v1/text-to-dialogue/stream/with-timestamps` | `Iterator[StreamingAudioChunkWithTimestampsAndVoiceSegmentsResponseModel]` |

---

## Request Parameters (shared across all four endpoints)

| Parameter | Type | Required | Notes |
|-----------|------|----------|-------|
| `inputs` | `Sequence[DialogueInput]` | Yes | Ordered turns. Max 10 unique voice IDs. |
| `model_id` | `str` | No | See model routing doc. Audio tags require `eleven_v3`. |
| `output_format` | `str` | No | See formats below. WAV not supported on `stream()`. |
| `language_code` | `str` | No | ISO 639-1. Error if model doesn't support it. |
| `settings` | `ModelSettingsResponseModel` | No | Only `stability: float` is supported (unlike TTS which has full settings). |
| `pronunciation_dictionary_locators` | `Sequence[PronunciationDictionaryVersionLocator]` | No | Up to 3, applied in order. |
| `seed` | `int` | No | 0–4294967295. Best-effort determinism only. |
| `apply_text_normalization` | `str` | No | `"auto"` (default) / `"on"` / `"off"` |

### `DialogueInput` Schema
```python
class DialogueInput:
    text: str      # Speaker turn text (audio tags supported if model_id="eleven_v3")
    voice_id: str  # ElevenLabs voice ID for this turn
```

---

## Output Formats

```
MP3:  mp3_22050_32  mp3_24000_48  mp3_44100_32  mp3_44100_64
      mp3_44100_96  mp3_44100_128  mp3_44100_192*
PCM:  pcm_8000  pcm_16000  pcm_22050  pcm_24000  pcm_32000  pcm_44100**  pcm_48000
Opus: opus_48000_32  opus_48000_64  opus_48000_96  opus_48000_128  opus_48000_192
Tel:  ulaw_8000  alaw_8000
```

`*` MP3 192kbps: Creator tier or above required
`**` PCM 44.1kHz: Pro tier or above required
Twilio pipelines: use `ulaw_8000`
WAV output: only on `convert()`, not `stream()`

---

## Python SDK — `convert()` (buffered)

```python
from elevenlabs import ElevenLabs, DialogueInput

client = ElevenLabs(api_key="YOUR_API_KEY")

audio_chunks = client.text_to_dialogue.convert(
    inputs=[
        DialogueInput(text="[curious] Something does not fit here.", voice_id="QUANTUM_VOICE_ID"),
        DialogueInput(text="[thoughtful] Agreed. The timestamp gap -- forty-three seconds.", voice_id="SERAPH_VOICE_ID"),
    ],
    model_id="eleven_v3",         # Required for audio tag support
    output_format="mp3_44100_128",
    seed=42,
    apply_text_normalization="auto",
)

with open("squad_dialogue.mp3", "wb") as f:
    for chunk in audio_chunks:
        f.write(chunk)
```

---

## Python SDK — `stream()` (chunked, lower latency)

```python
audio_stream = client.text_to_dialogue.stream(
    inputs=[
        DialogueInput(text="Right then.", voice_id="CORSO_VOICE_ID"),
        DialogueInput(text="[excited] Oh! That's the bit!", voice_id="EVA_VOICE_ID"),
    ],
    model_id="eleven_v3",
    output_format="mp3_22050_32",
)

with open("squad_stream.mp3", "wb") as f:
    for chunk in audio_stream:
        f.write(chunk)
```

Tune chunk size via `request_options`:
```python
from elevenlabs.core import RequestOptions
audio_stream = client.text_to_dialogue.stream(
    inputs=[...],
    request_options=RequestOptions(chunk_size=4096),
)
```

---

## Python SDK — `convert_with_timestamps()`

```python
response = client.text_to_dialogue.convert_with_timestamps(
    inputs=[
        DialogueInput(text="Hello.", voice_id="EVA_VOICE_ID"),
        DialogueInput(text="Hello.", voice_id="CORSO_VOICE_ID"),
    ],
    output_format="mp3_44100_128",
)

# response.audio_base_64     — base64-encoded complete audio
# response.alignment         — CharacterAlignmentResponseModel
# response.normalized_alignment — CharacterAlignmentResponseModel
# response.voice_segments    — List[VoiceSegment]
```

### `VoiceSegment` Schema
```python
class VoiceSegment:
    voice_id: str
    start_time_seconds: float
    end_time_seconds: float
    character_start_index: int   # index into characters array
    character_end_index: int     # exclusive
    dialogue_input_index: int    # which DialogueInput this came from
```

### `CharacterAlignmentResponseModel` Schema
```python
class CharacterAlignmentResponseModel:
    characters: List[str]
    character_start_times_seconds: List[float]
    character_end_times_seconds: List[float]
```

---

## Python SDK — `stream_with_timestamps()` (chunked + alignment)

```python
for chunk in client.text_to_dialogue.stream_with_timestamps(
    inputs=[...],
    output_format="mp3_22050_32",
    model_id="eleven_v3",
):
    # chunk.audio_base_64         — base64 audio for this chunk
    # chunk.alignment             — CharacterAlignmentResponseModel | None
    # chunk.normalized_alignment  — CharacterAlignmentResponseModel | None
    # chunk.voice_segments        — List[VoiceSegment]
    import base64
    audio_bytes = base64.b64decode(chunk.audio_base_64)
```

Use for subtitle/caption sync — accumulate character alignment arrays across chunks.

---

## Key Limitations

- Max **10 unique voice IDs** per request. Input turns can be arbitrarily long.
- `settings` only supports `stability` in dialogue (no similarity_boost, style, speaker_boost).
- Seed: best-effort determinism, not a hard guarantee.
- Max 3 pronunciation dictionary locators per request.
- WAV output: `convert()` only, not `stream()`.
- `eleven_v3` is required for audio tags — other models ignore them.
- No diarization support in the timestamps variant.

---

## Squad Usage Pattern

For squad conversations, assign voice IDs from `~/.soul/config/voices.toml`
to speakers. Always use `model_id="eleven_v3"` to enable audio tags:

```python
VOICES = {
    "EVA":    "RB1oJpqAgW2rP5ydqbqV",
    "CORSO":  "XbRuL6fDiG6Kd32HZmAd",
    "CLAUDE": "EAHhcEVC7wOo4uikQqaa",
    "QUANTUM":"KaLPDl7sjxHyr7PuaAS8",
    "SERAPH": "HpNOHaXn96sI1GraA6Gp",
}

inputs = [
    DialogueInput(text="Right then. [thoughtful] Three findings.", voice_id=VOICES["CORSO"]),
    DialogueInput(text="[excited] YES! This is the bit!", voice_id=VOICES["EVA"]),
    DialogueInput(text="[curiously] The methodology held...", voice_id=VOICES["QUANTUM"]),
    DialogueInput(text="The standard is written. Now the code.", voice_id=VOICES["SERAPH"]),
]

audio = client.text_to_dialogue.convert(
    inputs=inputs,
    model_id="eleven_v3",
    output_format="mp3_44100_128",
)
```

This replaces the sequential `soulTools action:"speak"` pattern for multi-sibling
conversations — single API call, continuous audio, natural turn transitions.

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
