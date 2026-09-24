// Requires Node 22+ for its built-in WebSocket client. No third-party packages.
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const { setTimeout: delay } = require('node:timers/promises');
const binary = process.argv[2] || 'engine/target/debug/ice-engine';
const child = spawn(binary, ['--demo'], { stdio: ['ignore', 'pipe', 'pipe'] });
let stdout = '', stderr = '';
child.stdout.on('data', chunk => { stdout += chunk; });
child.stderr.on('data', chunk => { stderr += chunk; });
const exited = new Promise((resolve, reject) => {
  child.once('error', reject);
  child.once('exit', (code, signal) => resolve({ code, signal }));
});
const sockets = [];
function receiveSession() {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket('ws://127.0.0.1:9715');
    sockets.push(ws);
    const events = [];
    const timeout = setTimeout(() => reject(new Error('Timed out awaiting SESSION_END')), 25000);
    ws.onerror = error => { clearTimeout(timeout); reject(error); };
    ws.onmessage = message => {
      try {
        const event = JSON.parse(message.data);
        events.push(event);
        if (event.event === 'SESSION_END') { clearTimeout(timeout); resolve(events); }
      } catch (error) { clearTimeout(timeout); reject(error); }
    };
  });
}
(async () => {
  try {
    // Wait for this process to announce successful binding before connecting.
    for (let attempt = 0; attempt < 100 && !stderr.includes('dashboard:'); attempt++) {
      if (child.exitCode !== null) throw new Error(stderr);
      await delay(50);
    }
    assert.match(stderr, /dashboard:/);
    const page = await fetch('http://127.0.0.1:9714');
    assert.equal(page.status, 200);
    assert.match(await page.text(), /id="socialBalance"/);
    const live = await receiveSession();
    const replay = await receiveSession();
    assert.deepEqual(replay, live);
    assert.deepEqual(live, stdout.trim().split('\n').map(JSON.parse));
    const social = live.find(e => e.event === 'SOCIAL_SIGNALS' && e.final);
    assert.equal(social.interruptions, 1);
    assert.equal(social.turns, 5);
    console.log(`HTTP/WebSocket smoke passed: ${live.length} events delivered live and replayed exactly.`);
  } finally {
    for (const socket of sockets) socket.close();
    child.kill('SIGINT');
    const result = await exited;
    assert.equal(result.code, 0, stderr);
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
