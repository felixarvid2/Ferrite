Ferrite-mcp is the edition of Ferrite meant to be driven by AI clients over
MCP (Model Context Protocol). Linux x86_64 and arm64. Chromium 158 (CEF 158.0.6).

New in 0.5.0: Chromium 156 -> 158 with its security fixes, the `url` and
`cookie` crate fixes, and arm64 packages.

It installs next to Ferrite and keeps its own profile, so the two can run
side by side.

## How it differs from Ferrite

- AI control (MCP) is on from the start, and so is the `evaluate` tool
  (custom JavaScript). Both can be turned off in Settings > AI control.
- No hand-over step: every web tab in a normal window can be controlled by
  the AI right away. Tabs the AI opens land in the normal window and use the
  normal profile, so a login you do for the AI stays. The Stop button on the
  AI bar (or the tab menu) takes a tab back; "Let AI control this tab" gives
  it back again.
- The MCP server starts the browser by itself when it is not running.
- Its own profile (`~/.local/share/ferrite-mcp`), window class and AppArmor
  profile; it does not register itself as a web browser.
- Still never reachable by the AI: ferrite:// pages, Settings, private
  windows, downloads, permission prompts and Proton Pass in controlled tabs.

## Downloads

- **`ferrite-mcp_*_amd64.deb`** (Ubuntu 24.04 / Debian). Recommended.
  `sudo apt install ./ferrite-mcp_*_amd64.deb`. Then add the MCP server to
  your client, for example Claude Code:
  `claude mcp add ferrite-mcp -- /usr/bin/ferrite-mcp-server`, or in
  `claude_desktop_config.json` / `.mcp.json`:
  `{ "mcpServers": { "ferrite-mcp": { "command": "/usr/bin/ferrite-mcp-server" } } }`.
  `ferrite-mcp-browser` (or "Ferrite-mcp" in the app menu) opens it by hand.
- **`Ferrite-mcp-*-x86_64.AppImage`**: a single file. Use
  `/path/to/Ferrite-mcp-*.AppImage --mcp-server` as the MCP command. On
  Ubuntu 23.10+ its sandbox cannot start (user namespaces are restricted);
  use the .deb there.
- **`ferrite-mcp_*_arm64.deb`**: the same for arm64; checksums in
  `SHA256SUMS-arm64`.
- `SHA256SUMS`: checksums of the x86_64 packages.

Everything else (ad blocking, tabs) is the same as Ferrite of the same
version; Ferrite-mcp keeps the Kladd look and the tab sidebar, while Ferrite
now looks like Firefox. The browser needs a graphical session (X11 or Wayland).
