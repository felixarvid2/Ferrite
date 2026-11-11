#!/usr/bin/env node
// Evaluate JS in a web tab whose URL contains <url-substring>; prints JSON. Waits up to TAB_TIMEOUT_MS (15s).
// Usage: tab.js <url-substring> "<js>"
// Note: background tabs may be lazy (no browser until activated); activate first via api('activate_tab',{tab}).
const { connect, pagesOf, out } = require('./cdp-lib');
const [sub, js] = process.argv.slice(2);
if (!sub || sub === '-h' || sub === '--help' || js === undefined) {
  console.log('Usage: tab.js <url-substring> "<js expression>"   (env: FERRITE_PORT, TAB_TIMEOUT_MS)'); process.exit(sub && sub.startsWith('-') ? 0 : 2);
}
const timeout = +(process.env.TAB_TIMEOUT_MS || 15000);
(async () => {
  const b = await connect(); const t0 = Date.now(); let p;
  for (;;) {
    p = pagesOf(b).find(p => !p.url().startsWith('ferrite://app/chrome.html') && p.url().includes(sub));
    if (p || Date.now() - t0 > timeout) break;
    await new Promise(r => setTimeout(r, 200));
  }
  if (!p) { console.error('no tab matching', sub, '; pages:', pagesOf(b).map(p => p.url())); process.exit(1); }
  out(await p.evaluate(js));
  process.exit(0);
})().catch(e => { console.error(e.message); process.exit(1); });
