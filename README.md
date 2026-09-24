# Ice Signal

A privacy-first social signal processor. Ice Signal understands the dynamics of
real-life conversations — balance, energy, pauses, interruptions — without ever
recording, storing, or transcribing a word.

**Privacy invariant:** raw audio is transient in capture/resampling memory and
is discarded after processing. The portable core retains only scalar state,
never an audio frame buffer. Only derived scalar metadata (JSON events)
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
    live meter, channel-based social estimates, and session analytics. None of
    this ports.
- `docs/event-schema.md` — the metadata event contract shared by the engine,
  dashboard, and app

## Running the engine

```sh
cd engine
cargo run --release            # JSONL events on stdout, live meter on stderr
cargo run --release 2>/dev/null  # events only (pipe to jq, a dashboard, …)
```

While the engine runs it also serves the live dashboard at
**http://localhost:9714** (event stream: `ws://localhost:9715`). The page is a
single self-contained file (`engine/src/host/dashboard.html`, embedded in the
binary) showing live stat tiles, the voice-activity timeline, the energy chart,
the event stream, and the privacy proof; the Conversation Card fills in at
session end. The WS stream carries exactly the JSONL schema events — the
dashboard sees precisely what the phone app would see, nothing more.

Grant your terminal microphone access when macOS prompts. The meter shows the
live level, the tracked noise floor, LIF spikes (⚡), and the VAD state; stdout
streams schema events (`SPEAKING_START`, `ENERGY_LEVEL`, …). Ctrl+C emits
`SESSION_ANALYTICS`, final `SOCIAL_SIGNALS`, `PRIVACY_SUMMARY`, and `SESSION_END`,
and prints the human summary including
`audio persisted: 0 bytes`.

## Social signals (Stage 6)

```sh
cd engine
cargo run --release -- --demo          # 20s synthetic conversation; no microphone
cargo run --release -- --dual-channel  # live input channels 1/2 = participants A/B
cargo run --release -- --demo --fast --headless  # deterministic JSONL smoke run
```

Open **http://localhost:9714** during the demo. After playback, the demo keeps
serving the completed card until Ctrl+C. `--headless` disables the server and
live meter; `--fast` is only valid with `--demo`. Use `--help` for all options.

The dashboard and terminal show estimated A/B speaking balance, completed
turns, overlap duration, possible interruptions, and a follow-up suggestion.
A/B are anonymous **input channel positions**, not recognized voices. Live
mode requires a default input device with at least two channels, with each
participant's close microphone routed separately to channels 1 and 2. Additional
channels are ignored for social signals. Ordinary stereo room microphones and
duplicated mono channels do not provide reliable participant separation.

The default mono mode still provides the existing session analytics. It reports
speaker-dependent signals as **unavailable**, with a general reflection prompt.
There are no voiceprints, speaker recognition, transcripts, inferred emotions,
or claims about interest. Microphone bleed, background noise, and VAD errors
can distort the estimates. Suggested next steps are optional conversation
prompts, not judgments about participants.

See [Stage 6 implementation and acceptance](docs/stage-6.md) and the
[event contract](docs/event-schema.md) for thresholds, units, and limitations.

### Verification

From the repository root:

```sh
cargo test --manifest-path engine/Cargo.toml
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path engine/Cargo.toml -- --check
cargo build --manifest-path engine/Cargo.toml
python3 engine/tests/demo_smoke.py engine/target/debug/ice-engine
node engine/tests/dashboard.test.cjs
node engine/tests/transport_smoke.cjs engine/target/debug/ice-engine # Node 22+, ~20s
```

## Roadmap (8 weeks)

1. ~~**Rust CLI engine** — mic input, 20ms frames, RMS energy, terminal output~~ ✓
2. ~~**VAD + metadata** — speaking start/stop, silence, long pauses, JSONL event stream~~ ✓ (spiking-neuron VAD in the portable core)
3. ~~**Privacy proof** — no-audio/no-transcription counters, session privacy summary~~ ✓
4. ~~**Session analytics** — duration, speech ratio, turn count, energy trend, momentum~~ ✓ (`SESSION_ANALYTICS` event + terminal Conversation Card)
5. ~~**Local dashboard** — WebSocket stream, live event feed, energy meter, Conversation Card~~ ✓ (served by the engine at `localhost:9714`)
6. ~~**Social signals** — estimated speaking balance, overlap, interruption, follow-up suggestion~~ ✓ (explicit A/B input channels, mono unavailable state, live dashboard + demo)
7. **React Native app** — sessions, live signal screen, Conversation Card, Privacy Proof screen ← *next*
8. **Event mode** — event tagging, anonymous aggregate analytics, organizer report, pilot-ready demo
