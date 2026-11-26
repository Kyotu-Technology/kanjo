#!/usr/bin/env node
// Takes the documentation screenshots from a running Kanjo demo server.
//
//   kanjo demo --no-open                      # prints a login link
//   node web/scripts/screenshots.mjs <login link> [--out docs/screenshots] [--chrome chromium]
//
// The login link is the one-time "?token=" link the demo prints; a link with the
// access token works too (kanjo --home <dir> open prints one). Needs Chromium or
// Chrome and Node 22 or newer (built-in WebSocket); nothing is installed.
// The onboarding shot turns onboarding back on in the demo's settings for a moment
// and turns it off again, so run this only against a demo server.
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const out = resolve(opt('--out', new URL('../../docs/screenshots', import.meta.url).pathname));
const chromeBin = opt('--chrome', process.env.CHROME ?? 'chromium-browser');
const port = Number(opt('--debug-port', '9399'));
const login = args[0];
if (!login) {
  console.error('usage: node web/scripts/screenshots.mjs <login link printed by kanjo demo> [--out dir] [--chrome path]');
  process.exit(2);
}
const base = new URL('/', login).href;
let loads = 0;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const profile = mkdtempSync(join(tmpdir(), 'kanjo-shots-'));
const chrome = spawn(chromeBin, ['--headless=new', '--no-sandbox', '--disable-gpu', '--hide-scrollbars', '--force-prefers-reduced-motion', '--lang=en-US', `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, 'about:blank'], { stdio: 'ignore' });

let ws;
let seq = 0;
const pending = new Map();
function send(method, params = {}) {
  return new Promise((ok, fail) => {
    const id = ++seq;
    pending.set(id, (m) => (m.error ? fail(new Error(`${method}: ${m.error.message}`)) : ok(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
}
async function evaluate(expression) {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  return r.result.value;
}
async function connect() {
  for (let i = 0; i < 100; i++) {
    try {
      const page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === 'page');
      if (page) {
        ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((r, j) => ((ws.onopen = r), (ws.onerror = j)));
        ws.onmessage = (e) => {
          const m = JSON.parse(e.data);
          if (m.id && pending.has(m.id)) {
            pending.get(m.id)(m);
            pending.delete(m.id);
          }
        };
        return;
      }
    } catch {
      // Chromium is still starting
    }
    await sleep(150);
  }
  throw new Error(`Chromium did not start (${chromeBin}); pass --chrome <path>`);
}

async function open(hash, { width = 1440, height = 900, mobile = false, theme = 'light' } = {}) {
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: mobile ? 2 : 1, mobile });
  await send('Page.navigate', { url: `${base}?lang=en&theme=${theme}&shot=${++loads}#${hash}` });
  await sleep(1500);
  for (let i = 0; i < 40; i++) {
    if (await evaluate("!document.querySelector('[aria-busy=\"true\"], .splash')")) break;
    await sleep(250);
  }
  await sleep(800);
}

/** Captures the whole page: the viewport grows to the content so sticky and 100vh parts stay in place. */
async function shot(name, { width = 1440, mobile = false, maxHeight = 3200, fullPage = true } = {}) {
  if (fullPage) {
    const h = await evaluate(`(() => {
      const full = document.documentElement.scrollHeight;
      if (full <= ${maxHeight}) return full;
      const ends = [...document.querySelectorAll('main .card')].map((c) => c.getBoundingClientRect().bottom + scrollY).filter((b) => b <= ${maxHeight});
      return Math.ceil(Math.max(900, ...ends) + 24);
    })()`);
    await send('Emulation.setDeviceMetricsOverride', { width, height: h, deviceScaleFactor: mobile ? 2 : 1, mobile });
    await sleep(500);
  }
  const { data } = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(out, `${name}.png`), Buffer.from(data, 'base64'));
  console.log(`  ${name}.png`);
}

const setOnboarding = (done) =>
  evaluate(`fetch('api/v1/settings').then((r) => r.json()).then((s) => fetch('api/v1/settings', { method: 'PUT', headers: { 'Content-Type': 'application/json', 'X-Kanjo-CSRF': '1' }, body: JSON.stringify({ ...s, onboarding_done: ${done} }) })).then((r) => r.status)`);

try {
  mkdirSync(out, { recursive: true });
  await connect();
  await send('Page.enable');
  await send('Runtime.enable');
  await send('Page.navigate', { url: login });
  await sleep(1500);
  const status = await evaluate("fetch('api/v1/settings').then((r) => r.status)");
  if (status !== 200) throw new Error(`the login link did not sign in (HTTP ${status}); one-time links work once, within two minutes`);
  console.log(`Writing to ${out}`);

  const pages = [
    ['dashboard-light', '/', {}],
    ['dashboard-dark', '/', { theme: 'dark' }],
    ['limits', '/limits', {}],
    ['usage', '/usage', {}],
    ['sessions', '/sessions', {}],
    ['insights', '/insights', {}],
    ['alerts-budgets', '/alerts', {}],
    ['system-privacy', '/system', {}],
  ];
  for (const [name, hash, o] of pages) {
    await open(hash, o);
    await shot(name);
  }
  await open('/', { width: 390, height: 844, mobile: true });
  await shot('dashboard-mobile', { width: 390, mobile: true, maxHeight: 2600 });

  if ((await setOnboarding(false)) !== 200) throw new Error('could not turn onboarding on for its screenshot');
  try {
    await open('/');
    await shot('onboarding', { fullPage: false });
  } finally {
    await setOnboarding(true);
  }
} catch (e) {
  console.error(String(e.message ?? e));
  process.exitCode = 1;
} finally {
  ws?.close();
  chrome.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
