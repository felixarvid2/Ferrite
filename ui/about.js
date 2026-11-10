"use strict";
api("about").then(a => {
  ver.textContent = "Version " + a.ferrite;
  const rows = [["Ferrite", a.ferrite], ["Chromium", a.chromium], ["CEF", a.cef], ["Profile directory", a.profile_dir], ["Sandbox", a.sandbox ? "Enabled" : "DISABLED (--no-sandbox)"], ["User agent", navigator.userAgent]];
  info.replaceChildren(...rows.map(([k, v]) => el("div", { class: "row" }, el("div", { class: "k muted", text: k }),
    el("div", { class: "grow kbd", style: "user-select:text;word-break:break-all", text: v }))));
});
