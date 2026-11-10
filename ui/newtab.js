"use strict";
document.getElementById("f").addEventListener("submit", e => { e.preventDefault(); const v = q.value.trim(); if (v) api("open_url", { url: v }); });
api("top_sites").then(r => {
  priv.hidden = !r.private;
  const seen = new Set(), tiles = document.getElementById("tiles");
  for (const s of [...r.bookmarks.filter(b => b.url), ...r.sites]) {
    if (seen.has(s.url) || seen.size >= 12) continue; seen.add(s.url);
    const a = el("a", { class: "tile", href: s.url, title: s.url }, favImg(s.url), el("span", { text: s.title || hostOf(s.url) }));
    a.addEventListener("click", e => { e.preventDefault(); api("open_url", { url: s.url, background: e.ctrlKey || e.metaKey }); });
    tiles.append(a);
  }
});
