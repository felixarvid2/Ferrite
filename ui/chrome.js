// Browser UI strip: renders state pushed from Rust (ferrite.onState) and
// sends user actions through the ferrite:// API.
"use strict";
const $ = id => document.getElementById(id);
async function api(cmd, args) {
  const res = await fetch("/api/" + cmd + "?a=" + encodeURIComponent(JSON.stringify(args || {})), { method: "POST" });
  const data = await res.json().catch(() => null);
  if (!res.ok) throw new Error(data && data.error || res.statusText);
  return data;
}
const cmd = name => api("command", { name });
// The same page also runs as the vertical-tabs sidebar (chrome.html#side).
const SIDE = location.hash === "#side";
if (SIDE) document.documentElement.dataset.side = "";

let state = null;
let editing = false;        // user is typing in the address bar
let typed = "";             // what the user typed (without inline completion)
let suggestions = [];
let sel = -1;
let suggestSeq = 0;

const ICON = "ferrite://app/icon.svg";
const LOCK = '<svg viewBox="0 0 16 16"><rect x="3.5" y="7" width="9" height="6.5" rx="1.2"/><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2"/></svg>';
const INFO = '<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor"/><path d="M8 7.5v4M8 5v.1"/></svg>';
const ICON_SVG = '<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor"/><path d="M6 11V5h4.2M6 8h3.2"/></svg>';
const WARN = '<svg viewBox="0 0 16 16"><path d="M8 2.5 14 13H2z"/><path d="M8 6.5v3M8 11.2v.1"/></svg>';

// ---------------------------------------------------------------- height
let lastHeight = 0;
function reportHeight() {
  if (SIDE) return;
  const h = Math.ceil(document.body.getBoundingClientRect().height);
  if (h !== lastHeight && h > 0) { lastHeight = h; api("set_height", { height: h }).catch(() => {}); }
}
new ResizeObserver(reportHeight).observe(document.body);

// ---------------------------------------------------------------- render
function el(tag, cls, text) { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; }

// Sites without a favicon get a coloured letter, stable per host.
function letterFor(pageUrl) {
  let host = ""; try { host = new URL(pageUrl).hostname.replace(/^www\./, ""); } catch (_) {}
  const d = el("span", "letter", (host[0] || "•"));
  let h = 0; for (const c of host) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  d.style.background = `hsl(${h % 360} 45% 42%)`;
  return d;
}
function favicon(url, pageUrl) {
  if (pageUrl && pageUrl.startsWith("ferrite:")) url = ICON;
  // Unloaded (restored or sleeping) tabs have no reported favicon yet: try
  // the site's conventional /favicon.ico before falling back to a letter.
  if (!url && /^https?:/.test(pageUrl || "")) { try { url = new URL(pageUrl).origin + "/favicon.ico"; } catch (_) {} }
  if (!url) return letterFor(pageUrl);
  const img = document.createElement("img");
  img.alt = "";
  img.src = url;
  img.onerror = () => img.replaceWith(letterFor(pageUrl));
  return img;
}

function renderTabs() {
  const box = $("tabs");
  const existing = new Map([...box.children].map(c => [Number(c.dataset.id), c]));
  const order = [];
  for (const t of state.tabs) {
    let node = existing.get(t.id);
    if (!node) node = makeTab(t.id);
    existing.delete(t.id);
    node.classList.toggle("active", t.id === state.active);
    node.classList.toggle("pinned", t.pinned);
    node.classList.toggle("closing", t.closing);
    node.classList.toggle("sleeping", !!t.sleeping);
    let host = ""; try { host = new URL(t.url).hostname.replace(/^www\./, ""); } catch (_) {}
    const title = t.title || (t.url === "ferrite://newtab/" ? "New Tab" : host || t.url) || "New Tab";
    node.title = title;
    node.querySelector(".tab-title").textContent = title;
    const icon = node.querySelector(".tab-icon");
    const want = t.loading ? "loading" : (t.crashed ? "crashed" : (t.favicon || "letter:" + t.url));
    if (icon.dataset.want !== want) {
      icon.dataset.want = want;
      icon.replaceChildren(t.loading ? el("div", "spinner") : t.crashed ? Object.assign(el("span"), { innerHTML: WARN }) : favicon(t.favicon, t.url));
    }
    node.querySelector(".tab-media").hidden = !t.media;
    order.push(node);
  }
  for (const n of existing.values()) n.remove();
  order.forEach((n, i) => { if (box.children[i] !== n) box.insertBefore(n, box.children[i] || null); });
}

let dragId = null;
function makeTab(id) {
  const n = el("div", "tab");
  n.dataset.id = id;
  n.setAttribute("role", "tab");
  n.draggable = true;
  n.append(el("span", "tab-icon"), el("span", "tab-title"), Object.assign(el("span", "tab-media", "●"), { title: "Using camera or microphone" }));
  const close = el("button", "tab-close");
  close.title = "Close tab (Ctrl+W)";
  close.innerHTML = '<svg viewBox="0 0 16 16"><path d="m3 3 10 10M13 3 3 13"/></svg>';
  close.addEventListener("click", e => { e.stopPropagation(); api("close_tab", { tab: id }); });
  n.append(close);
  n.addEventListener("mousedown", e => { if (e.button === 0) api("activate_tab", { tab: id }); });
  n.addEventListener("auxclick", e => { if (e.button === 1) { e.preventDefault(); api("close_tab", { tab: id }); } });
  n.addEventListener("contextmenu", e => { e.preventDefault(); api("tab_menu", { tab: id, x: Math.round(e.clientX), y: Math.round(e.clientY) }); });
  n.addEventListener("dblclick", e => e.preventDefault());
  n.addEventListener("dragstart", e => { dragId = id; e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", String(id)); });
  n.addEventListener("dragend", () => { dragId = null; clearDrop(); });
  n.addEventListener("dragover", e => {
    if (dragId == null || dragId === id) return;
    e.preventDefault();
    const r = n.getBoundingClientRect(); const left = SIDE ? e.clientY < r.top + r.height / 2 : e.clientX < r.left + r.width / 2;
    clearDrop(); n.classList.add(left ? "drag-over-left" : "drag-over-right");
  });
  n.addEventListener("drop", e => {
    e.preventDefault();
    if (dragId == null || dragId === id) return;
    const r = n.getBoundingClientRect(); const left = SIDE ? e.clientY < r.top + r.height / 2 : e.clientX < r.left + r.width / 2;
    const ids = state.tabs.map(t => t.id).filter(t => t !== dragId);
    let index = ids.indexOf(id) + (left ? 0 : 1);
    api("move_tab", { tab: dragId, index });
    clearDrop();
  });
  return n;
}
function clearDrop() { document.querySelectorAll(".drag-over-left,.drag-over-right").forEach(n => n.classList.remove("drag-over-left", "drag-over-right")); }

function renderNav() {
  let nav = state.nav || {};
  $("back").disabled = !nav.can_back;
  $("forward").disabled = !nav.can_forward;
  $("reload").classList.toggle("loading", !!nav.loading);
  $("reload").title = nav.loading ? "Stop (Esc)" : "Reload (Ctrl+R)";
  if (!editing) { $("address").value = nav.display_url || ""; paintAddressView(nav.display_url || ""); }
  const sec = $("security");
  if (!nav.display_url) nav = Object.assign({}, nav, { security: "" }); // New Tab page: no chip
  sec.className = nav.security || "";
  sec.innerHTML = { secure: LOCK, insecure: WARN + "Not secure", error: WARN + "Not private", internal: "", local: INFO + "File" }[nav.security] || "";
  sec.title = { secure: "Connection is secure (HTTPS)", insecure: "This connection is not encrypted", error: "The certificate of this site is not valid", local: "Local file" }[nav.security] || "";
  const zoom = $("zoom");
  zoom.hidden = !nav.zoom || nav.zoom === 100;
  zoom.textContent = (nav.zoom || 100) + "%";
  $("star").classList.toggle("on", !!nav.bookmarked);
  const bl = state.blocking || {};
  const shield = $("shield");
  shield.hidden = !(bl.global && bl.site);
  shield.classList.toggle("off", !bl.enabled);
  const badge = $("shield-badge");
  badge.hidden = !bl.enabled || !(bl.count > 0);
  badge.textContent = bl.count > 99 ? "99+" : String(bl.count || "");
  shield.title = !bl.enabled ? `Ad blocking is off on ${bl.site || "this site"}` : bl.count ? `${bl.count} blocked on ${bl.site}` : "Ad blocking is on";
  $("pass").hidden = !state.passwords;
  $("star").hidden = state.private || !nav.url || nav.url.startsWith("ferrite:");
  const p = $("progress");
  p.classList.toggle("on", !!nav.loading);
  p.style.width = nav.loading ? Math.max(8, Math.round((nav.progress || 0) * 100)) + "%" : "100%";
  // Find bar
  const find = nav.find || {};
  $("findbar").hidden = !find.open;
  if (find.open) {
    if (document.activeElement !== $("find-input") && $("find-input").value !== find.text) $("find-input").value = find.text || "";
    $("find-count").textContent = find.text ? (find.count ? `${find.active} of ${find.count}` : "No results") : "";
    $("find-count").classList.toggle("none", !!find.text && !find.count);
  }
  $("crashbar").hidden = !nav.crashed;
}

// Unfocused address bar: dim the scheme/path, show the host in full ink.
function paintAddressView(url) {
  const view = $("address-view");
  view.replaceChildren();
  $("address").classList.toggle("has-view", !!url);
  if (!url) return;
  let u; try { u = new URL(url); } catch (_) { view.textContent = url; return; }
  if (!/^https?:$/.test(u.protocol)) { view.append(el("b", null, url)); return; }
  const host = u.host;
  const i = url.indexOf(host);
  const pre = url.slice(0, i).replace(/^https:\/\//, ""), rest = url.slice(i + host.length);
  view.append(pre, el("b", null, host), rest === "/" ? "" : rest);
}

function renderBookmarks() {
  const bar = $("bookmarks");
  const items = state.bookmarks_bar;
  // An empty bar is noise: it appears with the first bookmark.
  bar.hidden = items == null || state.private || !items.length;
  if (bar.hidden) return;
  bar.replaceChildren();
  for (const b of items) {
    const n = el("div", "bm");
    n.title = b.url ? `${b.title}\n${b.url}` : b.title;
    if (b.kind === "folder") {
      n.append(Object.assign(el("span"), { innerHTML: '<svg viewBox="0 0 16 16"><path d="M2 4.5h4l1.5 1.5H14V12H2z"/></svg>' }), el("span", null, b.title));
      n.addEventListener("click", () => cmd("bookmarks"));
    } else {
      let fav; try { fav = new URL(b.url).origin + "/favicon.ico"; } catch (_) {}
      n.append(favicon(fav, b.url), el("span", null, b.title || b.url));
      n.addEventListener("click", e => api("open_url", { url: b.url, background: e.ctrlKey || e.metaKey }));
      n.addEventListener("auxclick", e => { if (e.button === 1) api("open_url", { url: b.url, background: true }); });
    }
    bar.append(n);
  }
}

function renderPrompts() {
  const box = $("prompts");
  box.replaceChildren();
  const names = { camera: "use your camera", microphone: "use your microphone", screen: "share your screen", geolocation: "know your location",
    notifications: "show notifications", clipboard: "read the clipboard", midi_sysex: "control MIDI devices", protected_media: "play protected content (DRM)",
    storage_access: "use cookies across sites", window_management: "manage windows on your screens", local_fonts: "use your local fonts",
    multiple_downloads: "download multiple files", keyboard_lock: "capture your keyboard", pointer_lock: "lock your mouse pointer", sensors: "use motion sensors" };
  for (const p of state.prompts || []) {
    const row = el("div", "prompt");
    let host = p.origin; try { host = new URL(p.origin).host; } catch (_) {}
    const what = p.permissions.map(x => names[x] || x.replace(/_/g, " ")).join(", ");
    const text = el("div", "grow what");
    const icon = el("span"); icon.innerHTML = '<svg viewBox="0 0 16 16"><path d="M8 1.8 13 4v4c0 3-2.2 5.3-5 6.2C5.2 13.3 3 11 3 8V4z"/></svg>';
    const words = el("span"); words.append(el("b", null, host), " wants to " + what);
    text.append(icon, words);
    const remember = Object.assign(document.createElement("input"), { type: "checkbox", checked: true });
    const label = el("label"); label.append(remember, "Remember");
    const allow = el("button", "pbtn primary", "Allow");
    const block = el("button", "pbtn", "Block");
    allow.onclick = () => api("permission", { id: p.id, allow: true, remember: remember.checked });
    block.onclick = () => api("permission", { id: p.id, allow: false, remember: remember.checked });
    row.append(text, label, block, allow);
    box.append(row);
  }
  // "Save password to Proton Pass?" (the password itself never reaches this page).
  const ps = state.pass_save;
  if (ps) {
    const row = el("div", "prompt");
    row.id = "pass-save";
    const text = el("div", "grow what");
    const icon = el("span"); icon.innerHTML = '<svg viewBox="0 0 16 16"><circle cx="5.5" cy="8" r="2.6" fill="none" stroke="currentColor"/><path d="M8.1 8H14M12 8v2.5M14 8v1.5"/></svg>';
    const words = el("span");
    words.append("Save password for ", el("b", null, ps.site), " to Proton Pass?");
    if (ps.username) words.append(" ", el("span", "muted", "(" + ps.username + ")"));
    text.append(icon, words);
    const answer = action => () => api("pass_save_answer", { id: ps.id, action });
    const save = el("button", "pbtn primary", "Save");
    const never = el("button", "pbtn", "Never for this site");
    const later = el("button", "pbtn", "Not now");
    save.onclick = answer("save"); never.onclick = answer("never"); later.onclick = answer("dismiss");
    row.append(text, never, later, save);
    box.append(row);
  }
}

function renderDownloads() {
  const d = state.downloads || {};
  const badge = $("dl-badge");
  badge.hidden = !d.active;
  badge.textContent = d.active ? (d.progress != null ? d.progress + "%" : d.active) : "";
}

window.ferrite = {
  onState(s) {
    state = s;
    document.documentElement.dataset.theme = s.theme === "light" ? "light"
      : s.theme === "system" && !matchMedia("(prefers-color-scheme: dark)").matches ? "light" : "dark";
    $("private-badge").hidden = !s.private;
    $("winctl").hidden = !s.frameless;
    if (s.frameless && !SIDE) document.documentElement.dataset.frameless = ""; else delete document.documentElement.dataset.frameless;
    if (s.vertical_tabs && !SIDE) document.documentElement.dataset.vtabs = ""; else delete document.documentElement.dataset.vtabs;
    $("win-max").title = s.maximized ? "Restore" : "Maximize";
    if (s.private) document.documentElement.dataset.private = ""; else delete document.documentElement.dataset.private;
    renderTabs(); renderNav(); renderBookmarks(); renderPrompts(); renderDownloads();
    reportHeight();
  },
  focusOmnibox() {
    if (SIDE) return;
    const a = $("address");
    a.focus(); a.select();
  },
  focusFind() {
    if (SIDE) return;
    $("findbar").hidden = false;
    const f = $("find-input"); f.focus(); f.select();
    reportHeight();
  },
  toast(msg) {
    if (SIDE) return;
    const t = $("toast"); t.textContent = msg; t.hidden = false;
    clearTimeout(t._h); t._h = setTimeout(() => t.hidden = true, 2200);
  },
};

// ---------------------------------------------------------------- toolbar
$("newtab").onclick = () => cmd("new_tab");
$("back").onclick = () => cmd("back");
$("forward").onclick = () => cmd("forward");
$("reload").onclick = e => cmd(state && state.nav && state.nav.loading ? "stop" : (e.shiftKey ? "hard_reload" : "reload"));
$("home").onclick = () => cmd("home");
$("star").onclick = () => api("toggle_bookmark").then(added => added != null && ferrite.toast(added ? "Bookmark added" : "Bookmark removed"));
$("shield").onclick = () => { const r = $("shield").getBoundingClientRect(); api("blocking_menu", { x: Math.round(r.right), y: Math.round(r.bottom) }); };
$("pass").onclick = () => { const r = $("pass").getBoundingClientRect(); api("password_menu", { x: Math.round(r.right), y: Math.round(r.bottom) }); };
$("downloads").onclick = () => cmd("downloads");
$("zoom").onclick = () => cmd("zoom_reset");
$("menu").onclick = () => { const r = $("menu").getBoundingClientRect(); api("menu", { x: Math.round(r.right), y: Math.round(r.bottom) }); };
$("crash-reload").onclick = () => cmd("reload");
// Double-click on empty strip: maximize like a title bar (frameless) or new tab.
$("tabstrip").addEventListener("dblclick", e => {
  if (!["tabstrip", "tabs", "drag-space"].includes(e.target.id)) return;
  if (state && state.frameless) api("window", { action: "maximize" }); else cmd("new_tab");
});
$("win-min").onclick = () => api("window", { action: "minimize" });
$("win-max").onclick = () => api("window", { action: "maximize" });
$("win-close").onclick = () => api("window", { action: "close" });

// ---------------------------------------------------------------- omnibox
const address = $("address");
address.addEventListener("focus", () => { setTimeout(() => address.select(), 0); });
address.addEventListener("blur", () => { setTimeout(() => { editing = false; hideSuggest(); if (state) renderNav(); }, 150); });
address.addEventListener("input", e => {
  editing = true;
  const deleting = e.inputType && e.inputType.startsWith("delete");
  typed = address.value;
  fetchSuggest(deleting);
});
address.addEventListener("keydown", e => {
  if (e.key === "Enter") {
    e.preventDefault();
    const item = sel >= 0 ? suggestions[sel] : null;
    const input = item ? item.url : address.value.trim();
    if (!input) return;
    editing = false; hideSuggest(); address.blur();
    api("navigate", { input, new_tab: e.altKey });
    api("focus_content");
  } else if (e.key === "Escape") {
    e.preventDefault();
    if (!$("suggest").hidden) { hideSuggest(); address.value = typed; return; }
    editing = false; renderNav(); address.select();
    api("focus_content");
  } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    if (!suggestions.length) return;
    e.preventDefault();
    sel = (sel + (e.key === "ArrowDown" ? 1 : -1) + suggestions.length + 1) % (suggestions.length + 1) - (0);
    if (sel >= suggestions.length) sel = -1;
    paintSuggest();
    address.value = sel >= 0 ? suggestions[sel].url : typed;
  }
});

function fetchSuggest(deleting) {
  const text = typed;
  const seq = ++suggestSeq;
  if (!text.trim()) { hideSuggest(); return; }
  api("suggest", { text }).then(r => {
    if (seq !== suggestSeq || !editing) return;
    suggestions = r.items || [];
    sel = -1;
    paintSuggest();
    // Inline completion: only while typing forward with the caret at the end.
    if (r.inline && !deleting && address.value === text && address.selectionStart === text.length) {
      address.value = text + r.inline;
      address.setSelectionRange(text.length, address.value.length);
    }
  }).catch(() => {});
}
function paintSuggest() {
  const box = $("suggest");
  box.style.setProperty("--sg-indent", Math.max(12, Math.round(address.getBoundingClientRect().left - 8 - 28)) + "px");
  box.replaceChildren();
  suggestions.forEach((s, i) => {
    const row = el("div", "sg" + (i === sel ? " sel" : ""));
    const k = el("span", "k");
    if (s.kind === "search") k.innerHTML = '<svg viewBox="0 0 16 16"><circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor"/><path d="m10.5 10.5 3 3"/></svg>';
    else if (s.kind === "bookmark") k.innerHTML = '<svg viewBox="0 0 16 16"><path d="m8 2 1.8 3.8 4.2.5-3.1 2.9.8 4.1L8 11.3l-3.7 2 .8-4.1L2 6.3l4.2-.5z"/></svg>';
    else k.innerHTML = '<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor"/><path d="M8 4.5V8l2.5 1.5"/></svg>';
    row.append(k, highlight(el("span", "t"), s.title || s.url, typed));
    if (s.kind !== "search") row.append(el("span", "u", s.url));
    row.addEventListener("mousedown", e => {
      e.preventDefault();
      editing = false; hideSuggest(); address.blur();
      api("navigate", { input: s.url, new_tab: e.button === 1 });
      api("focus_content");
    });
    box.append(row);
  });
  box.hidden = !suggestions.length;
  reportHeight();
}
function highlight(node, text, q) {
  const i = q ? text.toLowerCase().indexOf(q.toLowerCase()) : -1;
  if (i < 0) { node.textContent = text; return node; }
  node.append(text.slice(0, i), el("mark", null, text.slice(i, i + q.length)), text.slice(i + q.length));
  return node;
}
function hideSuggest() { suggestions = []; sel = -1; $("suggest").hidden = true; reportHeight(); }

// ---------------------------------------------------------------- find bar
const findInput = $("find-input");
let findTimer = 0;
findInput.addEventListener("input", () => {
  clearTimeout(findTimer);
  findTimer = setTimeout(() => api("find", { text: findInput.value, next: false }), 120);
});
findInput.addEventListener("keydown", e => {
  if (e.key === "Enter") { e.preventDefault(); api("find", { text: findInput.value, next: true, backward: e.shiftKey }); }
  else if (e.key === "Escape") { e.preventDefault(); api("find_close"); }
});
$("find-next").onclick = () => api("find", { text: findInput.value, next: true });
$("find-prev").onclick = () => api("find", { text: findInput.value, next: true, backward: true });
$("find-close").onclick = () => api("find_close");

document.addEventListener("contextmenu", e => { if (!e.target.closest("input")) e.preventDefault(); });

api("ready").then(s => ferrite.onState(s)).catch(e => console.error("ready failed", e));
