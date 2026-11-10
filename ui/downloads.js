"use strict";
async function load() {
  const items = await api("downloads_list");
  const f = q.value.trim().toLowerCase();
  list.replaceChildren();
  const shown = items.filter(d => !f || (d.file_name + " " + d.url).toLowerCase().includes(f));
  if (!shown.length) { list.append(el("div", { class: "empty", text: "Files you download appear here" })); return; }
  for (const d of shown) {
    const running = d.state === "in_progress";
    const status = running ? `${fmtBytes(d.received_bytes)} of ${d.total_bytes > 0 ? fmtBytes(d.total_bytes) : "?"}` + (d.speed > 0 ? ` — ${fmtBytes(d.speed)}/s` : "")
      : d.state === "complete" ? (d.exists ? fmtBytes(d.total_bytes || d.received_bytes) : "Deleted") : d.state === "cancelled" ? "Cancelled" : "Failed";
    const info = el("div", { class: "grow" },
      el("div", { class: "title", text: d.file_name }),
      el("div", { class: "sub", text: status + " · " + hostOf(d.url) }),
      d.dangerous && d.state === "complete" ? el("div", { class: "warn", text: "⚠ This type of file can harm your computer. Only open it if you trust its source." }) : null,
      running ? el("div", { class: "bar" }, el("div", { style: `width:${Math.max(2, d.percent || 0)}%` })) : null);
    const actions = [];
    if (running) actions.push(el("button", { class: "btn", text: "Cancel", onclick: () => api("download_cancel", { live_id: d.live_id }).then(load) }));
    if (d.state === "complete" && d.exists) {
      actions.push(el("button", { class: "btn", text: "Open", onclick: () => {
        if (d.dangerous && !confirm(`“${d.file_name}” may be harmful. Open it anyway?`)) return;
        api("download_open", { path: d.path }).catch(e => alert(e.message));
      } }));
      actions.push(el("button", { class: "btn", text: "Show in folder", onclick: () => api("download_open", { path: d.path, folder: true }).catch(e => alert(e.message)) }));
    }
    if (!running) actions.push(el("button", { class: "icon-btn", title: "Remove from list", text: "✕", onclick: () => api("download_remove", { id: d.id }).then(load) }));
    list.append(el("div", { class: "row" }, info, ...actions));
  }
}
clear.onclick = () => api("downloads_clear").then(load);
q.addEventListener("input", load);
load(); setInterval(load, 1000);
