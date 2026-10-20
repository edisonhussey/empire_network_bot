// Headless screenshots of a page that renders WebGL and sets window.__ready (core/tiles.html, core/index.html), through the Chrome
// DevTools protocol, so the GPU canvas is captured after the GLBs have loaded (plain `chrome --screenshot` fires before that).
//   node --experimental-websocket tools/shot.mjs <out.png> "<url>" [width height] [--wait=ms]
// Needs Google Chrome in /Applications and the static server (node serve.mjs 5174).
import { spawn } from 'node:child_process';
import { writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [out, url, w = '1600', h = '1000'] = process.argv.slice(2).filter((a) => !a.startsWith('--'));
const wait = +(process.argv.find((a) => a.startsWith('--wait='))?.slice(7) ?? 1500);
if (!out || !url) { console.error('usage: node --experimental-websocket tools/shot.mjs out.png "<url>" [w h] [--wait=ms]'); process.exit(2); }

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const port = 9300 + Math.floor(Math.random() * 500);
const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${port}`, `--window-size=${w},${h}`, '--hide-scrollbars', '--use-angle=metal',
  '--ignore-gpu-blocklist', `--user-data-dir=${mkdtempSync(join(tmpdir(), 'shot-'))}`, 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

try {
  let target;
  for (let i = 0; i < 50 && !target; i++) {
    await sleep(200);
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === 'page'); } catch {}
  }
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener('open', r, { once: true }));
  let id = 0;
  const pending = new Map();
  ws.addEventListener('message', (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Emulation.setDeviceMetricsOverride', { width: +w, height: +h, deviceScaleFactor: 1, mobile: false });
  await send('Page.enable');
  await send('Page.navigate', { url });
  for (let i = 0; i < 150; i++) {
    const r = await send('Runtime.evaluate', { expression: 'window.__ready === true', returnByValue: true });
    if (r.result?.result?.value) break;
    await sleep(200);
  }
  await sleep(wait);
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(out, Buffer.from(shot.result.data, 'base64'));
  console.log(`wrote ${out}`);
  ws.close();
} finally {
  chrome.kill();
}
