"use strict";
api("about").then(a => {
  const rows = [["Ferrite", a.ferrite], ["Chromium", a.chromium], ["CEF", a.cef], ["Profile directory", a.profile_dir], ["Sandbox", a.sandbox ? "enabled" : "DISABLED (--no-sandbox)"], ["User agent", navigator.userAgent]];
  info.replaceChildren(...rows.map(([k, v]) => el("div", { class: "row" }, el("div", { style: "width:160px", class: "muted", text: k }), el("div", { class: "grow", style: "user-select:text;word-break:break-all", text: v }))));
});
