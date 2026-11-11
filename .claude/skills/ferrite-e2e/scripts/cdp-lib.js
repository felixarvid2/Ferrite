// Shared by ui.js / tab.js: connect to Ferrite over CDP using port from env or the state file.
const fs = require('fs');
function req(name) {
  try { return require(name); } catch (e) {
    const root = require('child_process').execSync('npm root -g').toString().trim();
    return require(require('path').join(root, name));
  }
}
function state() {
  const f = process.env.FERRITE_E2E_STATE || `${process.env.TMPDIR || '/tmp'}/ferrite-e2e.state`;
  const o = {};
  try { for (const l of fs.readFileSync(f, 'utf8').split('\n')) { const m = l.match(/^(\w+)=(.*)$/); if (m) o[m[1]] = m[2]; } } catch {}
  return o;
}
const port = () => process.env.FERRITE_PORT || state().PORT || '9444';
async function connect() { return req('playwright').chromium.connectOverCDP(`http://127.0.0.1:${port()}`); }
const pagesOf = b => b.contexts().flatMap(c => c.pages());
const out = r => console.log(typeof r === 'string' ? r : JSON.stringify(r, null, 1));
module.exports = { connect, pagesOf, out, port };
