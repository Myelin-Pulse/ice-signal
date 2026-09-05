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

- `engine/` — Rust CLI engine, split to mirror the eventual hardware boundary:
  - `src/core/` — the FPGA-portable part: fixed-point only, no allocation, no
    floats, one `tick()` per sample. Energy accumulator → noise-gated LIF
    (leaky integrate-and-fire) spiking neuron → VAD state machine → per-frame
    report struct (the module's "output ports").
  - `src/host/` — the testbench part that stays on the computer/phone: cpal
    mic capture, resampling to the core's fixed 16 kHz, JSONL event output,
    live meter. None of this ports.
- `docs/event-schema.md` — the metadata event contract shared by the engine,
  dashboard, and app

## Running the engine

```sh
cd engine
cargo run --release            # JSONL events on stdout, live meter on stderr
cargo run --release 2>/dev/null  # events only (pipe to jq, a dashboard, …)
```

Grant your terminal microphone access when macOS prompts. The meter shows the
live level, the tracked noise floor, LIF spikes (⚡), and the VAD state; stdout
streams schema events (`SPEAKING_START`, `ENERGY_LEVEL`, …). Ctrl+C emits
`PRIVACY_SUMMARY` + `SESSION_END` and prints the human summary including
`audio persisted: 0 bytes`.

## Roadmap (8 weeks)

1. ~~**Rust CLI engine** — mic input, 20ms frames, RMS energy, terminal output~~ ✓
2. ~~**VAD + metadata** — speaking start/stop, silence, long pauses, JSONL event stream~~ ✓ (spiking-neuron VAD in the portable core)
3. ~~**Privacy proof** — no-audio/no-transcription counters, session privacy summary~~ ✓
4. **Session analytics** — duration, speech ratio, turn count, energy trend, momentum ← *here*
5. **Local dashboard** — WebSocket stream, live event feed, energy meter, Conversation Card
6. **Social signals** — estimated speaking balance, overlap, interruption, follow-up suggestion
7. **React Native app** — sessions, live signal screen, Conversation Card, Privacy Proof screen
8. **Event mode** — event tagging, anonymous aggregate analytics, organizer report, pilot-ready demo
