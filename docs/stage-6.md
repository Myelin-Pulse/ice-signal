# Stage 6 — social signals

Implemented against the repository's Stage 6 roadmap and append-only event
contract. The external design-artifact link in the README was inaccessible
while implementing; no additional acceptance criteria could be recovered there.

## Architecture decision

A mixed mono energy stream cannot identify who is speaking or prove that two
people are speaking simultaneously. Stage 6 therefore adds an explicit
`--dual-channel` mode for two separately routed participant microphones. This
preserves the no-voiceprint boundary and permits reproducible measurements.
Mono remains the default and reports unavailable attribution, never fabricated
balance or overlap. Supporting attribution from a single room microphone is
not part of this implementation.

The fixed-point core algorithms are unchanged. The host runs one core for each
participant channel and one for the existing mixed-channel analytics. The host
emits anonymous activity transitions; `SocialSignals` consumes only those
metadata events. No raw audio, spectra, embeddings, or names reach analytics,
the WebSocket stream, or the dashboard. Metadata history remains in memory for
replay. Host capture and resampling still use transient sample chunks; this is
not a guarantee that the entire host retains only one 20ms frame.

## Acceptance

- [x] Per-channel activity time and normalized A/B speaking balance.
- [x] Completed turns and overlap episodes, with documented duration thresholds.
- [x] Possible interruptions require overlapping speech and sustained takeover.
- [x] Deterministic follow-up prompts, with a low-evidence fallback.
- [x] Explicit unavailable state for mono; rejection of dual mode on mono devices.
- [x] Live dashboard, JSONL/WebSocket metadata, and final terminal/card output.
- [x] Synthetic demo exercises the production cores and emits five channel turns,
      one overlap, and one possible interruption without microphone access.
- [x] Silence, handoffs, short overlap, backchannels, simultaneous starts,
      interrupted takeovers, and shutdown are covered by automated tests.
- [x] Event documentation, runnable smoke checks, and roadmap updated.

## Validation and practical limits

Run the commands in the README. The demo smoke check parses every JSONL line,
checks monotonic times and final ordering, asserts the known social outcomes,
checks privacy counters, and verifies invalid CLI usage. The dashboard check
executes the embedded JavaScript against a minimal DOM and verifies live,
final, mono, and replay states. It is not a browser layout test. The transport
smoke check runs the real-time demo and verifies HTTP serving, live WebSocket
delivery, and an exact late-client replay against stdout. Ports 9714/9715 must
be free for this check.

Real microphone routing and accuracy need a manual check with two separately
routed microphones: alternate speaking, overlap, then let the newcomer
continue. Watch A/B shares, overlap and interruption counts, and end the
session with Ctrl+C. Microphone bleed, noise, short hesitations and the
energy-based VAD can all cause errors. Generic stereo input is not sufficient
unless its channels actually isolate participants. No real microphone
accuracy or user study is claimed by the synthetic acceptance tests.

Stage 7 can consume `SOCIAL_CONFIG` and `SOCIAL_SIGNALS` directly for the mobile
live screen and Conversation Card. It should retain the estimated/unavailable
labels and avoid treating A/B as persistent identities.
