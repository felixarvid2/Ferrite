"use strict";
function tick() {
  const d = new Date(), h = d.getHours();
  clock.textContent = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  greet.textContent = (h < 5 ? "Good night" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening") + " · " +
    d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });
}
tick(); setInterval(tick, 10000);
document.getElementById("f").addEventListener("submit", e => { e.preventDefault(); const v = q.value.trim(); if (v) api("open_url", { url: v }); });
// Show the current search engine in the placeholder.
api("settings_get").then(r => {
  const id = r.settings.search_engine;
  const e = r.engines.find(x => x.id === id);
  const name = id === "custom" ? "your search engine" : e && e.name;
  if (name) q.placeholder = `Search ${name} or type an address`;
}).catch(() => {});
api("top_sites").then(r => {
  priv.hidden = !r.private;
  if (r.private) {
    document.documentElement.dataset.private = "";
    document.title = "New Private Tab";
    badge.hidden = false; badge.replaceChildren(icon("eye"), "Private browsing");
    privh.replaceChildren(icon("shield"), "You are browsing privately");
  }
  const seen = new Set(), tiles = document.getElementById("tiles");
  for (const s of [...r.bookmarks.filter(b => b.url), ...r.sites]) {
    const key = hostOf(s.url).replace(/^www\./, "");
    if (seen.has(key) || seen.size >= 12) continue; seen.add(key);
    const a = el("a", { class: "tile", href: s.url, title: s.url }, el("div", { class: "ic" }, favImg(s.url)), el("span", { class: "name", text: s.title || hostOf(s.url) }));
    a.addEventListener("click", e => { e.preventDefault(); api("open_url", { url: s.url, background: e.ctrlKey || e.metaKey }); });
    tiles.append(a);
  }
});
