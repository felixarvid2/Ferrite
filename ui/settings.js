"use strict";
let settings;
async function load() {
  const r = await api("settings_get");
  settings = r.settings;
  const sel = document.getElementById("engines");
  sel.replaceChildren(...r.engines.map(e => el("option", { value: e.id, text: e.name })), el("option", { value: "custom", text: "Custom…" }));
  for (const input of document.querySelectorAll("[data-key]")) {
    const v = settings[input.dataset.key];
    if (input.type === "checkbox") input.checked = !!v; else input.value = String(v);
  }
  renderExceptions();
  loadPerms();
  loadPass();
  loadImport();
  const a = await api("about");
  document.getElementById("about").textContent = `Ferrite ${a.ferrite} · Chromium ${a.chromium} · CEF ${a.cef}` + (a.sandbox ? "" : " · sandbox disabled");
}
function renderExceptions() {
  const box = document.getElementById("adblock-sites");
  const sites = settings.adblock_exceptions || [];
  box.replaceChildren(...(sites.length ? [el("div", { class: "row" }, el("div", { class: "grow" }, el("div", { text: "Sites where blocking is off" }), el("div", { class: "desc", text: "Ads and trackers are allowed on these sites and their subdomains." })))] : []),
    ...sites.map(site => el("div", { class: "row hoverable" },
      el("div", { class: "grow" }, el("div", { class: "title", text: site })),
      el("button", { class: "icon-btn danger", title: "Turn blocking back on", "aria-label": "Remove " + site, onclick: async () => {
        settings = await api("settings_set", { patch: { adblock_exceptions: sites.filter(x => x !== site) } });
        renderExceptions();
      } }, icon("close")))));
}
async function loadPerms() {
  const perms = await api("permissions_list");
  const box = document.getElementById("perms");
  box.replaceChildren();
  if (!perms.length) { box.append(el("div", { class: "empty compact" }, el("b", { text: "No site permissions" }), el("div", { text: "No site has been given or refused permissions yet." }))); return; }
  for (const p of perms) box.append(el("div", { class: "row hoverable" },
    el("div", { class: "grow" }, el("div", { class: "title", text: p.origin }), el("div", { class: "sub", text: p.permission.replace(/_/g, " ") })),
    el("span", { class: "chip " + (p.decision === "allow" ? "ok" : "bad"), text: p.decision === "allow" ? "Allowed" : "Blocked" }),
    el("button", { class: "icon-btn danger", title: "Reset", "aria-label": "Reset", onclick: () => api("permission_remove", { origin: p.origin, permission: p.permission }).then(loadPerms) }, icon("close"))));
}
let passTimer = 0;
async function loadPass(refresh) {
  clearTimeout(passTimer);
  let r;
  try { r = await api(refresh ? "pass_refresh" : "pass_status"); } catch (e) { return; }
  const text = { disabled: "Disabled", not_installed: "pass-cli not found", loading: "Loading logins…",
    error: "Error: " + r.message, ready: `Ready: ${r.count} login${r.count === 1 ? "" : "s"}` }[r.status] || r.status;
  const box = document.getElementById("pass-status");
  box.textContent = text;
  box.style.color = r.status === "error" || r.status === "not_installed" ? "var(--danger)" : "";
  document.getElementById("pass-refresh").disabled = !r.enabled;
  if (r.status === "loading") passTimer = setTimeout(loadPass, 800);
}
const fmtNum = n => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, " ");
async function loadImport() {
  const box = document.getElementById("import-sources");
  let list;
  try { list = await api("import_sources"); } catch (e) { list = []; }
  if (!list.length) {
    box.replaceChildren(el("div", { class: "empty compact" }, el("div", { text: "No Chrome, Chromium, Brave, Edge, Vivaldi or Firefox profiles found." })));
    return;
  }
  box.replaceChildren(...list.map(src => {
    const bm = el("input", { type: "checkbox" });
    const hi = el("input", { type: "checkbox" });
    bm.checked = src.has_bookmarks; bm.disabled = !src.has_bookmarks;
    hi.checked = src.has_history; hi.disabled = !src.has_history;
    const result = el("div", { class: "desc", role: "status" });
    const btn = el("button", { class: "btn sm", text: "Import" });
    btn.onclick = async () => {
      if (!bm.checked && !hi.checked) { result.textContent = "Choose bookmarks or history."; return; }
      const all = document.querySelectorAll("#import-sources .btn");
      all.forEach(b => b.disabled = true); result.textContent = "Importing…";
      try {
        await api("import_run", { source: src.id, bookmarks: bm.checked, history: hi.checked });
        result.textContent = await pollImport();
      } catch (e) { result.textContent = "Import failed: " + e.message; }
      all.forEach(b => b.disabled = false);
    };
    return el("div", { class: "row", "data-source": src.id },
      el("div", { class: "grow" }, el("div", { text: `${src.browser} · ${src.profile_name}` }), result),
      el("label", {}, bm, "Bookmarks"), el("label", {}, hi, "History"), btn);
  }));
}
async function pollImport() {
  for (;;) {
    const st = await api("import_status");
    if (st.state === "done") {
      const c = st.counts, parts = [];
      if (c.bookmarks || !c.history) parts.push(`${fmtNum(c.bookmarks)} bookmarks`);
      if (c.history) parts.push(`${fmtNum(c.history)} history entries`);
      return `Imported ${parts.join(" and ")}` + (c.skipped ? ` (${fmtNum(c.skipped)} already present, skipped)` : "") + ".";
    }
    if (st.state === "error") return "Import failed: " + st.message;
    await new Promise(r => setTimeout(r, 400));
  }
}
document.getElementById("pass-refresh").onclick = () => loadPass(true);
for (const input of document.querySelectorAll("[data-key]")) {
  input.addEventListener("change", async () => {
    const value = input.type === "checkbox" ? input.checked : ("number" in input.dataset ? Number(input.value) : input.value);
    try { settings = await api("settings_set", { patch: { [input.dataset.key]: value } }); }
    catch (e) { alert(e.message); load(); }
    if (input.dataset.key.startsWith("proton_pass")) loadPass();
    if (input.dataset.key === "theme") { try { localStorage.setItem("ferrite-theme", value); } catch (_) {} location.reload(); }
  });
}
document.getElementById("clearbtn").onclick = async () => {
  const g = id => document.getElementById(id).checked;
  const r = await api("clear_browsing_data", { history: g("c-history"), cookies: g("c-cookies"), cache: g("c-cache"), site_data: g("c-site"), downloads: g("c-downloads") });
  alert("Cleared: " + (r.cleared.join(", ") || "nothing"));
};
load();
