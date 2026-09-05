# Ice Signal metadata event schema

The engine emits a stream of JSON events, one per line (JSONL). This is the
only data that ever leaves the engine — every consumer (local dashboard,
mobile app, analytics layer) builds against this contract.

Design rules:

- **No content.** Events carry timings and scalar levels only — never samples,
  spectra, embeddings, or anything a voice or word could be reconstructed from.
- **Monotonic time.** `t` is milliseconds since session start, never wall-clock,
  so a session log alone cannot be correlated to a calendar moment.
- **Append-only.** New event types and fields may be added; existing fields are
  never repurposed.

## Envelope

```json
{"t": 12340, "event": "SPEAKING_START", ...}
```

| Field   | Type   | Meaning                                  |
| ------- | ------ | ---------------------------------------- |
| `t`     | int    | ms since `SESSION_START`                 |
| `event` | string | event type, `SCREAMING_SNAKE_CASE`       |

## Events

### Session lifecycle

- `SESSION_START` — `{ "frame_ms": 20, "engine_version": "0.1.0" }`
- `SESSION_END` — `{ "duration_ms": 1834000 }`

### Voice activity (Week 2)

- `SPEAKING_START` — voice activity began
- `SPEAKING_STOP` — `{ "utterance_ms": 3200 }`
- `SILENCE` — sustained quiet crossed the threshold, `{ "since_ms": 1500 }`
- `LONG_PAUSE` — conversational lull, `{ "pause_ms": 4000 }`

### Energy (Week 2+)

- `ENERGY_LEVEL` — periodic (e.g. 1s) coarse level,
  `{ "band": "LOW" | "MED" | "HIGH", "dbfs": -32.4 }`

### Social signals (Week 6)

- `OVERLAP_DETECTED` — simultaneous speech, `{ "overlap_ms": 400 }`
- `INTERRUPTION` — overlap that ended the current speaker's turn
- `TURN_TAKEN` — speaker turn boundary, `{ "turn_ms": 5100 }`

### Privacy proof (Week 3)

- `PRIVACY_SUMMARY` — emitted at session end:

```json
{
  "t": 1834000,
  "event": "PRIVACY_SUMMARY",
  "frames_processed": 91700,
  "audio_samples_discarded": 88032000,
  "audio_bytes_persisted": 0,
  "words_transcribed": 0
}
```

## Explicitly out of scope

The engine will never emit: raw samples, FFT/spectral data, speaker
embeddings/voiceprints, transcripts, or wall-clock timestamps. If a feature
seems to need one of these, the feature changes, not this list.
