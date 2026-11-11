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
  loadPerms();
  loadPass();
  const a = await api("about");
  document.getElementById("about").textContent = `Ferrite ${a.ferrite} · Chromium ${a.chromium} · CEF ${a.cef}` + (a.sandbox ? "" : " · sandbox disabled");
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
