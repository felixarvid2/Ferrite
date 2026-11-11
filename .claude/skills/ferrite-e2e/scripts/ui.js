#!/usr/bin/env node
// Evaluate a JS expression in the browser UI page (ferrite://app/chrome.html) and print the JSON result.
// Promises are awaited. Examples:
//   ui.js "api('state')"      ui.js "api('command',{name:'new_tab'})"
//   ui.js --wait              (block until chrome.html exists and api is a function; exit 1 on timeout)
// Window selection: FERRITE_WIN=<n> picks the n-th UI page (0-based) when several windows are open.
// Port: FERRITE_PORT or the ferrite-up.sh state file.
const { connect, pagesOf, out } = require('./cdp-lib');
const a = process.argv.slice(2);
if (!a.length || a[0] === '-h' || a[0] === '--help') {
  console.log('Usage: ui.js "<js expression>" | ui.js --wait   (env: FERRITE_PORT, FERRITE_WIN, UI_TIMEOUT_MS)'); process.exit(a.length ? 0 : 2);
}
const timeout = +(process.env.UI_TIMEOUT_MS || 30000);
(async () => {
  const b = await connect();
  const t0 = Date.now(); let ui;
  for (;;) {
    const uis = pagesOf(b).filter(p => p.url().includes('ferrite://app/chrome.html'));
    ui = uis[+(process.env.FERRITE_WIN || 0)];
    if (ui) { try { if (await ui.evaluate(() => typeof api === 'function')) break; } catch {} }
    if (Date.now() - t0 > timeout) { console.error('UI page not ready; pages:', pagesOf(b).map(p => p.url())); process.exit(1); }
    await new Promise(r => setTimeout(r, 200));
  }
  if (a[0] === '--wait') process.exit(0);
  out(await ui.evaluate(a[0]));
  process.exit(0);
})().catch(e => { console.error(e.message); process.exit(1); });
