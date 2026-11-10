"use strict";
const EXT = {
  image: "png jpg jpeg gif webp svg bmp ico avif heic", archive: "zip tar gz tgz xz bz2 7z rar zst iso",
  doc: "pdf doc docx odt txt md rtf xls xlsx ods csv ppt pptx epub", media: "mp3 mp4 mkv webm wav flac ogg m4a mov avi",
  app: "exe msi deb rpm appimage sh bat apk dmg run",
};
function kindOf(name) {
  const ext = (name.split(".").pop() || "").toLowerCase();
  for (const [k, v] of Object.entries(EXT)) if (v.split(" ").includes(ext)) return k;
  return "file";
}
function chip(d, running) {
  if (running) return el("span", { class: "chip info", text: "Downloading" });
  if (d.state === "complete") return d.exists ? el("span", { class: "chip ok", text: "Complete" }) : el("span", { class: "chip", text: "Deleted" });
  if (d.state === "cancelled") return el("span", { class: "chip", text: "Cancelled" });
  return el("span", { class: "chip bad", text: "Failed" });
}
async function load() {
  const items = await api("downloads_list");
  const f = q.value.trim().toLowerCase();
  const shown = items.filter(d => !f || (d.file_name + " " + d.url).toLowerCase().includes(f));
  const cards = [];
  for (const d of shown) {
    const running = d.state === "in_progress";
    const status = running ? `${fmtBytes(d.received_bytes)} of ${d.total_bytes > 0 ? fmtBytes(d.total_bytes) : "?"}` + (d.speed > 0 ? ` \u2014 ${fmtBytes(d.speed)}/s` : "")
      : d.state === "complete" ? (d.exists ? fmtBytes(d.total_bytes || d.received_bytes) : "File was moved or deleted") : "";
    const kind = kindOf(d.file_name);
    const actions = [];
    if (running) actions.push(el("button", { class: "btn sm", onclick: () => api("download_cancel", { live_id: d.live_id }).then(load) }, icon("close"), "Cancel"));
    if (d.state === "complete" && d.exists) {
      actions.push(el("button", { class: "btn sm", onclick: () => {
        if (d.dangerous && !confirm(`\u201c${d.file_name}\u201d may be harmful. Open it anyway?`)) return;
        api("download_open", { path: d.path }).catch(e => alert(e.message));
      } }, icon("file"), "Open"));
      actions.push(el("button", { class: "btn sm", onclick: () => api("download_open", { path: d.path, folder: true }).catch(e => alert(e.message)) }, icon("folderOpen"), "Show in folder"));
    }
    cards.push(el("div", { class: "row dl hoverable" + (d.state === "complete" && !d.exists ? " gone" : "") },
      el("div", { class: "ftype " + kind }, icon(kind === "file" ? "file" : kind)),
      el("div", { class: "grow" },
        el("div", { class: "head" }, el("div", { class: "title", text: d.file_name }), chip(d, running)),
        el("div", { class: "sub", text: [status, hostOf(d.url)].filter(Boolean).join(" \u00b7 ") }),
        d.dangerous && d.state === "complete" ? el("div", { class: "warnbox" }, icon("alert"), "This type of file can harm your computer. Only open it if you trust its source.") : null,
        running ? el("div", { class: "bar" }, el("div", { style: `width:${Math.max(2, d.percent || 0)}%` })) : null,
        actions.length ? el("div", { class: "btns" }, ...actions) : null),
      running ? null : el("div", { class: "actions" }, el("button", { class: "icon-btn danger", title: "Remove from list", "aria-label": "Remove from list", onclick: () => api("download_remove", { id: d.id }).then(load) }, icon("close")))));
  }
  list.replaceChildren(shown.length ? el("div", { class: "card" }, ...cards)
    : el("div", { class: "card" }, f ? emptyState("search", "No matching downloads", "Try a different search term.") : emptyState("download", "No downloads yet", "Files you download appear here.")));
}
clear.onclick = () => api("downloads_clear").then(load);
q.addEventListener("input", load);
load(); setInterval(load, 1000);
