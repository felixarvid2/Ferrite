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
  const rgb = priv ? "183,155,255" : light ? "65,87,230" : "124,140,255";
  const cy = Math.min(h * 0.33, 300), span = Math.min(w * 0.28, 380);
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
  const lines = 30;
  for (let i = 0; i < lines; i++) {
    const a = (i / lines) * Math.PI * 2;
    let x = poles[0].x + Math.cos(a) * 14, y = poles[0].y + Math.sin(a) * 14;
    g.beginPath(); g.moveTo(x, y);
    for (let step = 0; step < 900; step++) {
      const [fx, fy] = field(x, y);
      x += fx * 4; y += fy * 4;
      g.lineTo(x, y);
      if (Math.hypot(x - poles[1].x, y - poles[1].y) < 12 || x < -50 || x > w + 50 || y < -50 || y > h + 50) break;
    }
    g.strokeStyle = `rgba(${rgb},${light ? 0.16 : 0.2})`;
    g.lineWidth = 1.2;
    g.stroke();
  }
  // Two faint pole glints.
  for (const p of poles) {
    const r = g.createRadialGradient(p.x, p.y, 0, p.x, p.y, 40);
    r.addColorStop(0, `rgba(${rgb},${light ? 0.18 : 0.28})`); r.addColorStop(1, `rgba(${rgb},0)`);
    g.fillStyle = r; g.beginPath(); g.arc(p.x, p.y, 40, 0, Math.PI * 2); g.fill();
  }
}
let fieldTimer = 0;
addEventListener("resize", () => { clearTimeout(fieldTimer); fieldTimer = setTimeout(drawField, 120); });
new MutationObserver(drawField).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "data-private"] });
drawField();
