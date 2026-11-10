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
function favImg(url) {
  const img = el("img", { class: "fav", src: faviconFor(url), alt: "" });
  img.onerror = () => { img.onerror = null; img.src = "ferrite://app/icon.svg"; };
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
// Apply the theme from settings to internal pages.
api("settings_get").then(r => {
  const t = r.settings.theme;
  const dark = t === "dark" || (t === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}).catch(() => {});
