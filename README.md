# Ice Signal

A privacy-first social signal processor. Ice Signal understands the dynamics of
real-life conversations — balance, energy, pauses, interruptions — without ever
recording, storing, or transcribing a word.

**Privacy invariant:** raw audio exists only inside the single 20ms frame being
processed, then it is discarded. Only derived scalar metadata (JSON events)
leaves the engine. Nothing is transcribed. Nothing touches disk.

**Design doc:** the full blueprint (architecture, mechanisms, non-goals, open
decisions, roadmap with acceptance criteria) lives at
https://claude.ai/code/artifact/c61e1329-0aa5-4c98-8cd5-ece864ef39ba

## Architecture

```
Mic ──▶ Rust Engine ──▶ Metadata API ──▶ Local Dashboard (demo)
        (frames in,              ├─────▶ Mobile App (product)
         audio discarded,        └─────▶ Event Analytics Layer
         events out)
```

- `engine/` — Rust CLI engine: live mic capture, 20ms frame processing, RMS
  energy, VAD and metadata events (in progress per roadmap)
- `docs/event-schema.md` — the metadata event contract shared by the engine,
  dashboard, and app

## Running the engine

```sh
cd engine
cargo run --release
```

Grant your terminal microphone access when macOS prompts. You'll see a live
energy meter and a running count of discarded audio; Ctrl+C prints the session
summary including `audio persisted: 0 bytes`.

## Roadmap (8 weeks)

1. **Rust CLI engine** — mic input, 20ms frames, RMS energy, terminal output ← *here*
2. **VAD + metadata** — speaking start/stop, silence, long pauses, JSONL event stream
3. **Privacy proof** — no-audio/no-transcription counters, session privacy summary
4. **Session analytics** — duration, speech ratio, turn count, energy trend, momentum
5. **Local dashboard** — WebSocket stream, live event feed, energy meter, Conversation Card
6. **Social signals** — estimated speaking balance, overlap, interruption, follow-up suggestion
7. **React Native app** — sessions, live signal screen, Conversation Card, Privacy Proof screen
8. **Event mode** — event tagging, anonymous aggregate analytics, organizer report, pilot-ready demo
