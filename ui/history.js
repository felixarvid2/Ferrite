"use strict";
async function load() {
  const visits = await api("history_search", { q: q.value, limit: 500 });
  list.replaceChildren();
  count.textContent = visits.length ? `${visits.length}${visits.length >= 500 ? "+" : ""} ${visits.length === 1 ? "visit" : "visits"}` : "Pages you have visited";
  if (!visits.length) { list.append(el("div", { class: "card" }, q.value ? emptyState("search", "No matching pages", "Try a different search term.") : emptyState("history", "No history yet", "Pages you visit appear here."))); return; }
  let day = null, card = null;
  for (const v of visits) {
    const d = fmtDay(v.visited_at);
    if (d !== day) { day = d; list.append(el("div", { class: "date-head", text: d })); card = el("div", { class: "card" }); list.append(card); }
    const open = e => { e.preventDefault(); api("open_url", { url: v.url, background: e.ctrlKey || e.metaKey }); };
    card.append(el("div", { class: "row hoverable" },
      el("span", { class: "time", text: fmtTime(v.visited_at) }), favImg(v.url),
      el("div", { class: "grow" }, el("a", { href: v.url, class: "title", text: v.title || v.url, onclick: open }), el("div", { class: "sub", text: v.url })),
      el("div", { class: "actions" },
        el("button", { class: "icon-btn danger", title: "Remove from history", "aria-label": "Remove from history", onclick: async () => { await api("history_delete", { id: v.id }); load(); } }, icon("close")))));
  }
}
let t; q.addEventListener("input", () => { clearTimeout(t); t = setTimeout(load, 150); });
clear.onclick = async () => {
  const label = range.options[range.selectedIndex].text.toLowerCase();
  if (!confirm(`Clear browsing history from the ${label}?`)) return;
  await api("history_clear", { range: range.value }); load();
};
load();
