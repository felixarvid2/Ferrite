"use strict";
async function load() {
  const visits = await api("history_search", { q: q.value, limit: 500 });
  list.replaceChildren();
  if (!visits.length) { list.append(el("div", { class: "card empty", text: q.value ? "No matching pages" : "Pages you visit appear here" })); return; }
  let day = null, card = null;
  for (const v of visits) {
    const d = fmtDay(v.visited_at);
    if (d !== day) { day = d; list.append(el("div", { class: "date-head", text: d })); card = el("div", { class: "card" }); list.append(card); }
    const link = el("a", { href: v.url, class: "title", text: v.title || v.url, onclick: e => { e.preventDefault(); api("open_url", { url: v.url, background: e.ctrlKey || e.metaKey }); } });
    card.append(el("div", { class: "row" },
      el("span", { class: "muted", text: fmtTime(v.visited_at) }), favImg(v.url),
      el("div", { class: "grow" }, link, el("div", { class: "sub", text: v.url })),
      el("button", { class: "icon-btn", title: "Remove from history", text: "✕", onclick: async () => { await api("history_delete", { id: v.id }); load(); } })));
  }
}
let t; q.addEventListener("input", () => { clearTimeout(t); t = setTimeout(load, 150); });
clear.onclick = async () => {
  const label = range.options[range.selectedIndex].text.toLowerCase();
  if (!confirm(`Clear browsing history from the ${label}?`)) return;
  await api("history_clear", { range: range.value }); load();
};
load();
