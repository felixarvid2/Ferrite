"use strict";
api("about").then(a => {
  ver.textContent = "Version " + a.ferrite;
  const rows = [["Ferrite", a.ferrite], ["Chromium", a.chromium], ["CEF", a.cef], ["Profile directory", a.profile_dir], ["Sandbox", a.sandbox ? "Enabled" : "DISABLED (--no-sandbox)"], ["Media codecs", codecSummary()], ["User agent", navigator.userAgent]];
  info.replaceChildren(...rows.map(([k, v]) => el("div", { class: "row" }, el("div", { class: "k muted", text: k }),
    el("div", { class: "grow kbd", style: "user-select:text;word-break:break-all", text: v }))));
});

// What this build of Chromium can play. Standard CEF builds lack the
// proprietary codecs; see docs/CODECS.md for a build that has them.
function codecSummary() {
  const v = document.createElement("video");
  const can = t => v.canPlayType(t) !== "";
  const list = [["H.264", 'video/mp4; codecs="avc1.42E01E"'], ["AAC", 'audio/mp4; codecs="mp4a.40.2"'],
                ["VP9", 'video/webm; codecs="vp9"'], ["AV1", 'video/mp4; codecs="av01.0.05M.08"'], ["Opus", 'audio/webm; codecs="opus"']];
  return list.map(([n, t]) => `${n} ${can(t) ? "✓" : "✗"}`).join("   ");
}

// Update check (see docs/STATUS.md). Links open in a new tab; nothing is downloaded here.
function link(url, text, id) {
  const a = el("a", { href: url, text, id });
  a.addEventListener("click", e => { e.preventDefault(); api("open_url", { url, new_tab: true }); });
  return a;
}
function renderUpdate(st) {
  const box = document.getElementById("update");
  const u = st.update;
  const row = (...kids) => el("div", { class: "row" }, ...kids);
  const rows = [];
  if (u) {
    rows.push(row(el("div", { class: "grow" },
      el("div", { id: "update-title", text: `Ferrite ${u.version} is available` }),
      el("div", { class: "desc", text: `You are running ${st.current}. Ferrite does not install updates by itself.` }))));
    const links = [link(u.url, "Release page", "update-page")];
    if (u.deb_url) links.push(link(u.deb_url, "Debian package (.deb)", "update-deb"));
    if (u.appimage_url) links.push(link(u.appimage_url, "AppImage", "update-appimage"));
    rows.push(row(el("div", { class: "grow", style: "display:flex;gap:18px;flex-wrap:wrap" }, ...links)));
  } else {
    let text = "Ferrite is up to date";
    if (!st.last_checked_ms) text = st.failed ? "Could not check for updates" : "Ferrite has not checked for updates yet";
    else text += ` (checked ${new Date(st.last_checked_ms).toLocaleString([], { dateStyle: "medium", timeStyle: "short" })})`;
    rows.push(row(el("div", { class: "grow" },
      el("div", { id: "update-title", text }),
      st.failed && st.last_checked_ms ? el("div", { class: "desc", text: "The last check failed; showing the previous result." }) : null,
      !st.enabled ? el("div", { class: "desc", text: "Automatic checks are off (Settings > Privacy and security)." }) : null)));
  }
  const btn = el("button", { class: "btn", id: "update-check", text: st.checking ? "Checking…" : "Check now" });
  btn.disabled = st.checking;
  btn.onclick = checkNow;
  rows[rows.length - 1].append(btn);
  box.replaceChildren(...rows);
}
async function checkNow() {
  let st = await api("update_check");
  renderUpdate(st);
  while (st.checking) {
    await new Promise(r => setTimeout(r, 400));
    st = await api("update_status");
    renderUpdate(st);
  }
}
api("update_status").then(st => { renderUpdate(st); if (st.checking) checkNow(); });
