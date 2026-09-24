"""Run the real CLI without microphone or network; validate its public contract."""
import json
import subprocess
import sys

binary = sys.argv[1] if len(sys.argv) > 1 else 'engine/target/debug/ice-engine'
run = subprocess.run([binary, '--demo', '--fast', '--headless'], capture_output=True, text=True, check=True)
events = [json.loads(line) for line in run.stdout.splitlines()]
assert events[0]['event'] == 'SESSION_START'
assert events[1] == dict(t=0, event='SOCIAL_CONFIG', mode='DUAL_CHANNEL', estimated=True, demo=True)
assert all(a['t'] <= b['t'] for a, b in zip(events, events[1:]))
assert [e['event'] for e in events[-4:]] == ['SESSION_ANALYTICS', 'SOCIAL_SIGNALS', 'PRIVACY_SUMMARY', 'SESSION_END']
social = events[-3]
assert social['final'] and social['available'] and social['estimated']
assert (social['turns'], social['overlaps'], social['interruptions']) == (5, 1, 1)
assert social['suggestion'] == 'LEAVE_SPACE'
assert abs(sum(social['shares']) - 1) < 0.0001
assert all(0 < share < 1 for share in social['shares'])
assert 200 <= social['overlap_ms'] <= 1000
assert len([e for e in events if e['event'] == 'TURN_TAKEN']) == 5
assert len([e for e in events if e['event'] == 'INTERRUPTION']) == 1
assert events[-2]['audio_samples_discarded'] == 640000
assert events[-2]['frames_processed'] == 1000
assert events[-2]['audio_bytes_persisted'] == events[-2]['words_transcribed'] == 0
assert events[-1]['duration_ms'] == 20000
for event in events:
    assert not {'samples', 'audio', 'transcript', 'embedding', 'voiceprint', 'wall_time'} & event.keys()
for args in [['--fast'], ['--unknown']]:
    invalid = subprocess.run([binary, *args], capture_output=True, text=True)
    assert invalid.returncode != 0 and not invalid.stdout
help_result = subprocess.run([binary, '--help'], capture_output=True, text=True, check=True)
assert '--dual-channel' in help_result.stderr
print(f'CLI smoke passed: {len(events)} valid events, 5 turns, 1 overlap, 1 possible interruption.')
