"use strict";
let current = 1, dragged = null;
addfolder.append(icon("folderPlus"), "New folder");
addbm.append(icon("plus"), "Add bookmark");
function node(n, depth) {
  const isFolder = n.kind === "folder";
  const actions = [];
  if (n.id > 2) {
    actions.push(el("button", { class: "icon-btn", title: "Edit", "aria-label": "Edit", onclick: async () => {
      const title = prompt("Name", n.title); if (title == null) return;
      let url = n.url; if (!isFolder) { url = prompt("URL", n.url); if (url == null) return; }
      await api("bookmark_update", { id: n.id, title, url }); load();
    } }, icon("edit")));
    actions.push(el("button", { class: "icon-btn danger", title: "Delete", "aria-label": "Delete", onclick: async () => {
      if (isFolder && n.children.length && !confirm(`Delete folder \u201c${n.title}\u201d and everything in it?`)) return;
      await api("bookmark_delete", { id: n.id }); load();
    } }, icon("trash")));
  }
  const label = isFolder
    ? el("div", { class: "grow title" }, n.title, " ", el("span", { class: "count", text: n.children.length ? "\u00b7 " + n.children.length : "" }))
    : el("div", { class: "grow" }, el("a", { href: n.url, class: "title", text: n.title || n.url, onclick: e => { e.preventDefault(); api("open_url", { url: n.url, background: e.ctrlKey || e.metaKey }); } }), el("div", { class: "sub", text: n.url }));
  const row = el("div", { class: "row hoverable" + (isFolder && n.id === current ? " current" : ""), draggable: n.id > 2 ? "true" : null },
    isFolder ? el("span", { class: "folder-ic", style: "display:flex" }, icon("folder")) : favImg(n.url), label, el("div", { class: "actions" }, ...actions));
  if (isFolder) row.addEventListener("click", e => { if (!e.target.closest("button")) { current = n.id; document.querySelectorAll(".row.current").forEach(r => r.classList.remove("current")); row.classList.add("current"); } });
  row.addEventListener("dragstart", () => { dragged = n; });
  row.addEventListener("dragover", e => { if (dragged && dragged.id !== n.id) { e.preventDefault(); row.classList.add("drop"); } });
  row.addEventListener("dragleave", () => row.classList.remove("drop"));
  row.addEventListener("drop", async e => {
    e.preventDefault(); row.classList.remove("drop");
    if (!dragged) return;
    try {
      if (isFolder) await api("bookmark_move", { id: dragged.id, parent: n.id, index: 0 });
      else await api("bookmark_move", { id: dragged.id, parent: n.parent_id, index: n.position });
    } catch (err) { alert(err.message); }
    dragged = null; load();
  });
  const wrap = el("div", { class: isFolder ? "folder" + (depth === 0 ? " root" : "") : "" }, row);
  if (isFolder && n.children.length) wrap.append(el("div", { class: "children" }, n.children.map(c => node(c, depth + 1))));
  return wrap;
}
async function load() {
  const roots = await api("bookmarks_tree");
  tree.replaceChildren();
  const filter = q.value.trim().toLowerCase();
  const match = n => n.kind === "folder" ? (n.children = n.children.filter(match)).length > 0 || n.id <= 2
                                       : (n.title + " " + n.url).toLowerCase().includes(filter);
  let total = 0; const count = n => n.kind === "folder" ? n.children.forEach(count) : total++;
  for (const r of roots) { if (filter) match(r); count(r); tree.append(el("div", { class: "card" }, node(r, 0))); }
  if (!total) tree.prepend(el("div", { class: "card", style: "margin-bottom:12px" }, filter ? emptyState("search", "No matching bookmarks", "Try a different search term.") : emptyState("bookmark", "No bookmarks yet", "Add one with the button above, or use the star in the address bar.")));
}
addfolder.onclick = async () => { const t = prompt("Folder name"); if (t) { await api("bookmark_folder_add", { parent: current, title: t }); load(); } };
addbm.onclick = async () => {
  const url = prompt("URL"); if (!url) return;
  const title = prompt("Name", url); if (title == null) return;
  await api("bookmark_add", { parent: current, title, url }); load();
};
let t; q.addEventListener("input", () => { clearTimeout(t); t = setTimeout(load, 150); });
load();
