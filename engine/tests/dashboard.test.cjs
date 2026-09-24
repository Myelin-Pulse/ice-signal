// Dependency-free checks against the embedded dashboard's actual event reducer.
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const elements = new Map();
function element() {
  return {
    textContent: '', innerHTML: '', style: {}, hidden: false, children: [],
    parentElement: { clientWidth: 640 },
    setAttribute(key, value) { this[key] = value; },
    append(child) { this.children.push(child); },
    prepend(child) { this.children.unshift(child); },
    replaceChildren() { this.children = []; },
    get childElementCount() { return this.children.length; },
    get lastElementChild() { return { remove: () => this.children.pop() }; },
    querySelector() { return null; },
  };
}
const context = vm.createContext({
  document: {
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); },
    createElement: element, addEventListener() {}, querySelector() { return null; },
  },
  window: { addEventListener() {} }, location: { hostname: 'localhost' },
  WebSocket: class {}, setInterval() {}, setTimeout() {}, Date,
});
const html = readFileSync(join(__dirname, '../src/host/dashboard.html'), 'utf8');
vm.runInContext(html.match(/<script>([\s\S]*?)<\/script>/)[1], context);
const event = value => vm.runInContext(`onEvent(${JSON.stringify(value)})`, context);
event({ t: 0, event: 'SESSION_START' });
event({ t: 0, event: 'SOCIAL_CONFIG', mode: 'DUAL_CHANNEL', demo: true });
event({ t: 10000, event: 'SOCIAL_SIGNALS', available: true, shares: [0.75, 0.25], turns: 3, overlap_ms: 400, overlaps: 1, interruptions: 1, suggestion_text: 'Leave space.' });
assert.equal(elements.get('socialBalance').textContent, 'A 75% / B 25%');
assert.equal(elements.get('socialOverlap').textContent, '0.4s · 1 episodes');
assert.equal(elements.get('socialInterruptions').textContent, 1);
assert.equal(elements.get('balanceBar').hidden, false);
assert.match(elements.get('socialMode').textContent, /Synthetic demo/);
event({ t: 10000, event: 'SOCIAL_SIGNALS', final: true, available: true, shares: [0.75, 0.25], turns: 3, overlap_ms: 400, overlaps: 1, interruptions: 1, suggestion_text: 'Leave space.' });
event({ t: 10000, event: 'SESSION_ANALYTICS', duration_ms: 10000, speech_ratio: 0.5, utterances: 3, utterances_per_min: 18, longest_lull_ms: 1000, long_pauses: 0, avg_dbfs: -30, energy_trend: 'STEADY', momentum: 'STEADY' });
assert.equal(elements.get('card').children.at(-1).textContent, 'Follow-up: Leave space.');
event({ t: 10000, event: 'SESSION_END' });
assert.match(elements.get('statusText').textContent, /session ended/);
// Replayed/restarted sessions must clear prior results.
event({ t: 0, event: 'SESSION_START' });
event({ t: 0, event: 'SOCIAL_CONFIG', mode: 'MONO', demo: false });
event({ t: 0, event: 'SOCIAL_SIGNALS', available: false, shares: null, suggestion_text: 'Reflect together.' });
assert.equal(elements.get('socialBalance').textContent, 'Unavailable');
assert.equal(elements.get('socialInterruptions').textContent, '—');
assert.equal(elements.get('balanceBar').hidden, true);
assert.match(elements.get('card').innerHTML, /Appears when/);
assert.match(elements.get('socialMode').textContent, /Separate participant channels/);
assert.equal(elements.get('feed').childElementCount, 3);
console.log('Dashboard event, rendering, final-card, and replay checks passed.');
