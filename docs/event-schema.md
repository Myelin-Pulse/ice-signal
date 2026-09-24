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

### Session analytics (Week 4)

- `SESSION_ANALYTICS` — emitted once at session end, before `PRIVACY_SUMMARY`:

```json
{
  "t": 1834000,
  "event": "SESSION_ANALYTICS",
  "duration_ms": 1834000,
  "speech_ms": 924000,
  "speech_ratio": 0.50,
  "utterances": 46,
  "utterances_per_min": 1.5,
  "longest_lull_ms": 7500,
  "long_pauses": 3,
  "avg_dbfs": -33.4,
  "energy_trend": "RISING",
  "momentum": "GAINING"
}
```

Definitions (fixed, per the append-only rule):

- *utterance* — one continuous stretch of voiced activity by any speaker.
  Computed on the mixed channel; Stage 6 channel turns are separate metrics
  and do not change the definition of an utterance.
- `speech_ratio` — `speech_ms / duration_ms`, 0..1.
- `longest_lull_ms` — largest gap between utterances, including before the
  first and after the last.
- `energy_trend` — mean dBFS of the session's second half vs its first half:
  `RISING` / `FALLING` beyond ±2 dB, else `STEADY`.
- `momentum` — speech density of the second half vs the first half:
  `GAINING` / `FADING` beyond ±0.10, else `STEADY`.

### Social signals (Week 6)

All social events contain scalar metadata only. A/B denote input channels 1/2
for this session, never persistent speaker identities. The existing mixed-channel
voice-activity events and `SESSION_ANALYTICS` retain their definitions.

- `SOCIAL_CONFIG` — emitted immediately after `SESSION_START`:
  `{ "mode": "DUAL_CHANNEL" | "MONO", "estimated": true, "demo": false }`.
  `demo: true` marks synthesized input, including its privacy counters.
- `CHANNEL_ACTIVITY` — emitted only when the two-channel activity mask changes:
  `{ "active_a": true, "active_b": false }`. Activity uses the same independent
  fixed-point cores as the mixed input, gated by frame energy above the noise
  threshold. It bridges up to two low-energy frames (40ms), avoiding the 400ms
  VAD hangover tail. This is an activity estimate, not speaker recognition.
- `TURN_TAKEN` — emitted when a channel's continuous activity ends:
  `{ "speaker": "A" | "B", "turn_ms": 5100, "estimated": true }`.
  A turn is a channel activity interval; two channels can have turns open
  simultaneously. A pause followed by the same channel starts another turn.
- `OVERLAP_DETECTED` — emitted at the end of a simultaneous activity interval
  lasting **at least 200ms**: `{ "overlap_ms": 400, "estimated": true }`.
- `INTERRUPTION` — possible takeover: overlap lasted at least 200ms, the
  incumbent channel stopped, and the newcomer remained active alone for at
  least **300ms**:
  `{ "speaker": "B", "interrupted": "A", "overlap_ms": 400, "estimated": true }`.
  Emitted once on confirmation. Simultaneous starts, simultaneous stops, a
  shorter overlap, or the newcomer stopping first do not establish a takeover.
  This is a timing heuristic; it does not infer intent or conversational norms.
- `SOCIAL_SIGNALS` — a cumulative snapshot every 50 frames (1s), plus one
  final snapshot after `SESSION_ANALYTICS` and before `PRIVACY_SUMMARY`:

```json
{
  "t": 20000,
  "event": "SOCIAL_SIGNALS",
  "final": true,
  "available": true,
  "estimated": true,
  "reason": "SEPARATE_CHANNELS",
  "shares": [0.55, 0.45],
  "speech_ms": [8800, 7200],
  "turns": 5,
  "overlap_ms": 600,
  "overlaps": 1,
  "interruptions": 1,
  "suggestion": "LEAVE_SPACE",
  "suggestion_text": "Try leaving a short pause before responding so each person can finish."
}
```

`shares` and `speech_ms` are ordered `[A, B]`. Shares divide each channel's
activity time by the **sum of both channel activity times**, including overlap
in each channel. They sum to 1 when activity exists; `shares` is `null` before
any activity. `overlap_ms` includes all simultaneous activity, including short
intervals below 200ms; `overlaps` counts only completed qualifying intervals.
`turns` counts completed channel intervals. Open turns and overlap close at
session end, without inventing a takeover. Durations integrate half-open
intervals `[start, end)` using frame-index timestamps (20ms resolution).

In mono mode, `available` is false, `reason` is `SEPARATE_CHANNELS_REQUIRED`,
and `shares`, `speech_ms`, `turns`, `overlap_ms`, `overlaps`, and `interruptions`
are all `null`. No channel, overlap, turn, or interruption events are emitted.
Zero must not be used to imply a measurement that cannot be made.

Suggestions are deterministic, evaluated in priority order:

| Code | Condition | Prompt intent |
| --- | --- | --- |
| `REFLECT_TOGETHER` | Mono input | Ask each other how the conversation felt |
| `KEEP_LISTENING` | Duration <10s or summed channel activity <5s | Wait for more activity |
| `LEAVE_SPACE` | At least one confirmed possible interruption | Leave a pause before responding |
| `INVITE_OTHER` | Either channel has at least 70% of activity | Invite the quieter participant to share |
| `ASK_FOLLOW_UP` | Otherwise | Ask an open follow-up question |

No signal or suggestion is evidence of emotion, attraction, agreement, or intent.
Downstream clients can reproduce the social state from `CHANNEL_ACTIVITY`
transitions plus the event clock; neither samples nor core reports are required.

### Privacy proof (Week 3)

`frames_processed` counts completed mixed-channel core frames, not the sum
across the three cores. `audio_samples_discarded` counts input scalar samples
consumed by processing, across all device channels (two synthesized channels
in demo mode), including a trailing partial frame. Capture/resampling uses
transient host buffers; no raw audio is persisted or sent through the API.


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
