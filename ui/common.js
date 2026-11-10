// Shared helpers for ferrite:// pages. `api(cmd, args)` calls into Rust.
"use strict";
async function api(cmd, args) {
  const res = await fetch("/api/" + cmd + "?a=" + encodeURIComponent(JSON.stringify(args || {})), { method: "POST" });
  const data = await res.json();
  if (!res.ok) throw new Error(data && data.error || res.statusText);
  return data;
}
function el(tag, attrs, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) if (c != null) e.append(c.nodeType ? c : document.createTextNode(String(c)));
  return e;
}
function faviconFor(url) {
  try { const u = new URL(url); if (u.protocol.startsWith("http")) return u.origin + "/favicon.ico"; } catch (_) {}
  return "ferrite://app/icon.svg";
}
// Coloured letter avatar, used when a site has no favicon.
function avatarFor(url, cls) {
  const host = hostOf(url).replace(/^www\./, "");
  let h = 0; for (const c of host) h = (h * 31 + c.charCodeAt(0)) % 360;
  return el("span", { class: cls || "avatar", text: (host[0] || "?").toUpperCase(), style: `background:hsl(${h} 52% 46%)` });
}
function favImg(url, cls) {
  let ok = false; try { ok = new URL(url).protocol.startsWith("http"); } catch (_) {}
  if (!ok) return el("img", { class: cls || "fav", src: "ferrite://app/icon.svg", alt: "" });
  const img = el("img", { class: cls || "fav", src: faviconFor(url), alt: "", loading: "lazy" });
  img.onerror = () => { img.onerror = null; img.replaceWith(avatarFor(url, cls)); };
  return img;
}
function hostOf(url) { try { return new URL(url).host || url; } catch (_) { return url; } }
function fmtBytes(n) {
  if (!(n > 0)) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"]; let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return n.toFixed(i ? 1 : 0) + " " + u[i];
}
function fmtTime(ms) { return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }); }
function fmtDay(ms) {
  const d = new Date(ms), today = new Date(); today.setHours(0, 0, 0, 0);
  const day = new Date(d); day.setHours(0, 0, 0, 0);
  const diff = Math.round((today - day) / 86400000);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  return d.toLocaleDateString([], { weekday: "long", year: "numeric", month: "long", day: "numeric" });
}
// Stroke icons (16px grid), drawn with currentColor.
const ICONS = {
  history: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  bookmark: '<path d="M6 4h12v17l-6-4-6 4z"/>',
  download: '<path d="M12 4v11m0 0l-4-4m4 4l4-4M5 20h14"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 00.3 1.8l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.8-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.8.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.8 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.8l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.8.3H9a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.8-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.8V9a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
  edit: '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M13.5 6.5l4 4"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  folder: '<path d="M3 6a2 2 0 012-2h4l2 2h8a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>',
  folderPlus: '<path d="M3 6a2 2 0 012-2h4l2 2h8a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2z"/><path d="M12 10v6M9 13h6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  file: '<path d="M14 3H7a2 2 0 00-2 2v14a2 2 0 002 2h10a2 2 0 002-2V8z"/><path d="M14 3v5h5"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="1.5"/><path d="M21 16l-5-5-8 8"/>',
  archive: '<rect x="3" y="4" width="18" height="5" rx="1"/><path d="M5 9v10a1 1 0 001 1h12a1 1 0 001-1V9M10 13h4"/>',
  doc: '<path d="M14 3H7a2 2 0 00-2 2v14a2 2 0 002 2h10a2 2 0 002-2V8z"/><path d="M14 3v5h5M9 13h6M9 17h6"/>',
  media: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M10 9.5v5l4.5-2.5z"/>',
  app: '<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M9 9l3 3-3 3M13 15h3"/>',
  folderOpen: '<path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v1H7l-4 8z"/><path d="M3 18l4-8h15l-4 8z"/>',
  shield: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/><path d="M9 12l2 2 4-4"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  alert: '<path d="M12 4l10 17H2z"/><path d="M12 10v5M12 18h.01"/>',
  lock: '<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 018 0v3"/>',
  bolt: '<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>',
  palette: '<path d="M12 3a9 9 0 100 18c1.4 0 2-1 1.5-2-.6-1.2.2-2.5 1.6-2.5H17a4 4 0 004-4c0-5-4-9.5-9-9.5z"/><circle cx="7.5" cy="11" r="1"/><circle cx="10" cy="7" r="1"/><circle cx="15" cy="7.5" r="1"/>',
  power: '<path d="M12 3v9M6.3 6.8a8 8 0 1011.4 0"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
};
function icon(name) {
  const s = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  s.setAttribute("viewBox", "0 0 24 24"); s.setAttribute("class", "i"); s.setAttribute("aria-hidden", "true");
  s.innerHTML = ICONS[name] || "";
  return s;
}
function emptyState(ic, title, text) {
  return el("div", { class: "empty" }, el("div", { class: "empty-icon" }, icon(ic)), el("b", { text: title }), text ? el("div", { text }) : null);
}
// Shared sidebar for History / Bookmarks / Downloads / Settings / About. Fills <aside id="nav" data-active="...">.
const NAV = [["history", "History", "history"], ["bookmarks", "Bookmarks", "bookmark"], ["downloads", "Downloads", "download"], ["settings", "Settings", "settings"], ["about", "About Ferrite", "info"]];
document.addEventListener("DOMContentLoaded", () => {
  const nav = document.getElementById("nav"); if (!nav) return;
  nav.className = "sidebar";
  nav.append(el("a", { class: "brand", href: "ferrite://newtab/", title: "New Tab" }, el("img", { src: "ferrite://app/icon.svg", alt: "" }), "Ferrite"));
  for (const [id, label, ic] of NAV) nav.append(el("a", { class: "nav" + (nav.dataset.active === id ? " active" : ""), href: "ferrite://" + id + "/" }, icon(ic), label));
});
// Apply the theme from settings to internal pages (cached to avoid a flash on load).
function applyTheme(t) {
  const dark = t === "dark" || (t === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}
try { const c = localStorage.getItem("ferrite-theme"); if (c) applyTheme(c); } catch (_) {}
api("settings_get").then(r => {
  applyTheme(r.settings.theme);
  try { localStorage.setItem("ferrite-theme", r.settings.theme); } catch (_) {}
}).catch(() => {});
