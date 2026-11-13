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

// Field lines of a magnetic dipole whose poles flank the clock, rendered as
// short "iron filing" dashes. Static (no animation), redrawn on resize and
// theme change.
function drawField() {
  const cv = document.getElementById("field");
  if (!cv) return;
  const dpr = window.devicePixelRatio || 1;
  const w = innerWidth, h = innerHeight;
  cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr);
  const g = cv.getContext("2d");
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, w, h);
  const light = document.documentElement.dataset.theme === "light";
  const priv = document.body.classList.contains("private") || document.documentElement.hasAttribute("data-private");
  const rgb = priv ? "183,155,255" : light ? "102,116,138" : "125,139,160";
  const glow = priv ? "183,155,255" : light ? "184,101,15" : "240,180,76";
  const clockEl = document.getElementById("clock");
  const cr = clockEl ? clockEl.getBoundingClientRect() : null;
  const cy = cr && cr.height ? cr.top + cr.height / 2 : Math.min(h * 0.3, 260);
  const span = Math.min(w * 0.3, 400);
  const poles = [{ x: w / 2 - span, y: cy, q: 1 }, { x: w / 2 + span, y: cy, q: -1 }];
  const field = (x, y) => {
    let fx = 0, fy = 0;
    for (const p of poles) {
      const dx = x - p.x, dy = y - p.y, r2 = dx * dx + dy * dy + 30, r3 = r2 * Math.sqrt(r2);
      fx += p.q * dx / r3; fy += p.q * dy / r3;
    }
    const m = Math.hypot(fx, fy) || 1;
    return [fx / m, fy / m];
  };
  g.lineCap = "round";
  g.setLineDash([3, 7]);
  // Seed lines on the mid-line between the poles and trace both ways, so
  // both poles look the same (no starburst at the seeding pole).
  const trace = (x, y, dir) => {
    g.moveTo(x, y);
    for (let step = 0; step < 900; step++) {
      const [fx, fy] = field(x, y);
      x += fx * 4 * dir; y += fy * 4 * dir;
      g.lineTo(x, y);
      const end = dir > 0 ? poles[1] : poles[0];
      if (Math.hypot(x - end.x, y - end.y) < 22 || x < -50 || x > w + 50 || y < -50 || y > h + 50) break;
    }
  };
  g.strokeStyle = `rgba(${rgb},${light ? 0.34 : 0.32})`;
  g.lineWidth = 1.2;
  // Keep the clock clear: seeds start above/below it and spread outward.
  const clear = cr && cr.height ? cr.height / 2 + 28 : 70;
  const range = Math.min(h, 900) * 0.55;
  for (const side of [-1, 1]) {
    for (let i = 0; i < 7; i++) {
      const y = cy + side * (clear + Math.pow(i / 6, 1.5) * range);
      g.beginPath(); trace(w / 2, y, 1); trace(w / 2, y, -1); g.stroke();
    }
  }
  // Two faint pole glints.
  for (const p of poles) {
    const r = g.createRadialGradient(p.x, p.y, 0, p.x, p.y, 64);
    r.addColorStop(0, `rgba(${glow},${light ? 0.18 : 0.24})`); r.addColorStop(1, `rgba(${glow},0)`);
    g.fillStyle = r; g.beginPath(); g.arc(p.x, p.y, 64, 0, Math.PI * 2); g.fill();
  }
}
let fieldTimer = 0;
addEventListener("resize", () => { clearTimeout(fieldTimer); fieldTimer = setTimeout(drawField, 120); });
new MutationObserver(drawField).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "data-private"] });
drawField();
