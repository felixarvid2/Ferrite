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
