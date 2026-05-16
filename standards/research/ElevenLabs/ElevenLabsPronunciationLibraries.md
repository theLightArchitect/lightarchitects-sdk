# ElevenLabs — Pronunciation Dictionaries

> Researched: 2026-02-28 | Source: elevenlabs-python SDK (GitHub), official docs

---

## Overview

Pronunciation dictionaries let you override how ElevenLabs pronounces specific words or phrases.
Two rule types: **alias** (spelling substitution) and **phoneme** (IPA phonetic encoding).

Each dictionary is versioned. Mutations create new versions — the original is preserved.
Production calls must pin a `version_id` to get reproducible results.

---

## Rule Types

### Alias Rules

Replaces a string with a different spelling that the model knows how to pronounce.

```python
from elevenlabs.types import AliasRule

rule = AliasRule(
    string_to_replace="SERAPH",
    alias="Seer-aff",   # How you want it pronounced, spelled phonetically
    type="alias",
)
```

**Use when**: Acronyms, proper nouns, technical terms the model mispronounces. Simple
letter substitution that redirects to a phonetically predictable spelling.

### Phoneme Rules

Encodes pronunciation directly in IPA (International Phonetic Alphabet).

```python
from elevenlabs.types import PhonemeRule

rule = PhonemeRule(
    string_to_replace="QUANTUM",
    phoneme="ˈkwɒntəm",   # IPA transcription
    alphabet="ipa",         # Only "ipa" is currently supported — no CMU arpabet
    type="phoneme",
)
```

**Use when**: Alias approach isn't precise enough; accent-specific phoneme targeting;
technical pronunciation that standard spelling will always mispronounce.

**Alphabet**: Only `"ipa"` is currently supported. CMU arpabet is not available.

---

## Creating a Pronunciation Dictionary

```python
from elevenlabs import ElevenLabs
from elevenlabs.types import AliasRule, PhonemeRule

client = ElevenLabs(api_key="YOUR_API_KEY")

# Create with rules
dictionary = client.pronunciation_dictionary.add_from_rules(
    rules=[
        AliasRule(string_to_replace="EVA", alias="Ee-vah", type="alias"),
        AliasRule(string_to_replace="CORSO", alias="Kor-so", type="alias"),
        PhonemeRule(string_to_replace="SERAPH", phoneme="ˈsɛrəf", alphabet="ipa", type="phoneme"),
    ],
    name="Squad Pronunciation",
    description="Pronunciation overrides for squad sibling names",
)

print(f"Dictionary ID: {dictionary.id}")
print(f"Version ID: {dictionary.version_id}")
# Pin these — version_id changes on every mutation
```

---

## Versioning

Every mutation creates a new version. The dictionary ID stays stable; the `version_id` increments.

```python
# Add rules to existing dictionary — creates new version
updated = client.pronunciation_dictionary.add_rules_from_the_pronunciation_dictionary(
    pronunciation_dictionary_id="dict_abc123",
    rules=[
        AliasRule(string_to_replace="QUANTUM", alias="Kwan-tum", type="alias"),
    ],
)

print(f"New version: {updated.version_id}")
# Previous version still exists — callers using old version_id are unaffected
```

```python
# Remove rules — also creates new version
updated = client.pronunciation_dictionary.remove_rules_from_the_pronunciation_dictionary(
    pronunciation_dictionary_id="dict_abc123",
    rule_strings=["EVA"],   # List of string_to_replace values to remove
)
```

---

## Using in TTS Calls

Pass locators (dictionary_id + version_id pairs) to any TTS call. Max **3 locators per call**.

```python
from elevenlabs import ElevenLabs
from elevenlabs.types import PronunciationDictionaryVersionLocator

client = ElevenLabs(api_key="YOUR_API_KEY")

audio = client.text_to_speech.convert(
    voice_id="RB1oJpqAgW2rP5ydqbqV",   # EVA
    text="EVA, CORSO, and QUANTUM are investigating the SERAPH anomaly.",
    model_id="eleven_v3",
    pronunciation_dictionary_locators=[
        PronunciationDictionaryVersionLocator(
            pronunciation_dictionary_id="dict_abc123",
            version_id="v_001",   # PINNED — use specific version, not "latest"
        ),
    ],
)
```

### Locator Priority

When multiple dictionaries are provided, rules are applied **in order** — first locator wins
on conflicts. Locators are evaluated left-to-right.

```python
pronunciation_dictionary_locators=[
    PronunciationDictionaryVersionLocator(id="squad-names", version_id="v_003"),   # applied first
    PronunciationDictionaryVersionLocator(id="technical-terms", version_id="v_001"),
    PronunciationDictionaryVersionLocator(id="acronyms", version_id="v_002"),
]
# Max 3 — this is the limit
```

---

## Text-to-Dialogue Support

Pronunciation dictionaries work with the Text-to-Dialogue API as well:

```python
audio = client.text_to_dialogue.convert(
    inputs=[
        DialogueInput(text="SERAPH is watching port 8443.", voice_id=VOICES["SERAPH"]),
        DialogueInput(text="QUANTUM confirms — sixty-eight percent.", voice_id=VOICES["QUANTUM"]),
    ],
    model_id="eleven_v3",
    pronunciation_dictionary_locators=[
        PronunciationDictionaryVersionLocator(
            pronunciation_dictionary_id="dict_abc123",
            version_id="v_001",
        ),
    ],
)
```

Same max-3 limit applies.

---

## Production Rules

### Pin Version IDs

Never pass a dictionary without a `version_id`. The API does not offer a "latest version"
shorthand — if omitted, behaviour is undefined. Treat dictionaries like dependencies:
pin versions, test on upgrades.

```python
# BAD — version unspecified
PronunciationDictionaryVersionLocator(pronunciation_dictionary_id="dict_abc123")

# GOOD — version pinned
PronunciationDictionaryVersionLocator(
    pronunciation_dictionary_id="dict_abc123",
    version_id="v_001",
)
```

### Store IDs in voices.toml or voice-profiles

Dictionary IDs and version IDs belong in configuration, not source code:

```toml
# ~/.soul/config/voices.toml (or voice-profiles/{sibling}.toml)
[squad_pronunciation]
dictionary_id = "dict_abc123"
version_id    = "v_003"
# Bump version_id here when dictionary is updated — single source of truth
```

---

## Listing and Reading Dictionaries

```python
# List all dictionaries (paginated)
dictionaries = client.pronunciation_dictionary.get_pls_list_of_the_pronunciation_dictionaries_v1()
for d in dictionaries.pronunciation_dictionaries:
    print(f"{d.id}: {d.name} — latest version: {d.version_id}")

# Get a specific dictionary
dictionary = client.pronunciation_dictionary.get_a_pls_dictionary_for_voice(
    pronunciation_dictionary_id="dict_abc123",
)
print(f"Rules: {dictionary.rules}")
```

---

## Key Limitations

- Max **3 pronunciation dictionary locators** per TTS or dialogue call
- Only `"ipa"` alphabet is supported for phoneme rules (no CMU arpabet)
- Rules are string-match based — no regex, no partial matches
- Dictionaries are account-scoped — shared across all voices
- Version IDs must be explicitly pinned — no "latest" shorthand

---

## Squad Usage Pattern

One squad-wide dictionary containing name overrides for all siblings:

```python
SQUAD_DICT = {
    "pronunciation_dictionary_id": "dict_squad_abc123",
    "version_id": "v_003",   # Pin and update in voices.toml when changed
}

locators = [
    PronunciationDictionaryVersionLocator(**SQUAD_DICT),
]

# Pass to any TTS or dialogue call
audio = client.text_to_speech.convert(
    voice_id=VOICES["EVA"],
    text="EVA speaking with SERAPH and QUANTUM on the Moltbook case.",
    model_id="eleven_v3",
    pronunciation_dictionary_locators=locators,
)
```

This ensures consistent sibling name pronunciation across all TTS production calls,
regardless of which voice or model is used.

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
