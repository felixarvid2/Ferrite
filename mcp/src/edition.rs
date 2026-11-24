//! Compile-time edition constants: standard Ferrite or the "MCP edition"
//! (`mcp-edition` feature), a Ferrite meant to be driven only by AI clients.
//! The two coexist on one machine: they differ in every name and path below.

/// True in the MCP edition.
pub const MCP_EDITION: bool = cfg!(feature = "mcp-edition");
/// Product name (window titles, UI).
pub const APP_NAME: &str = if MCP_EDITION {
    "Ferrite-mcp"
} else {
    "Ferrite"
};
/// Linux application id (desktop file, Wayland app id).
pub const APP_ID: &str = if MCP_EDITION {
    "ferrite-mcp"
} else {
    "ferrite"
};
/// X11 WM_CLASS class.
pub const WM_CLASS: &str = if MCP_EDITION {
    "Ferrite-mcp"
} else {
    "Ferrite"
};
/// Directory name under the platform data dir.
pub const DATA_DIR_NAME: &str = if MCP_EDITION {
    "ferrite-mcp"
} else {
    "ferrite"
};
/// Prefix of GitHub release tags that belong to this edition.
pub const RELEASE_TAG_PREFIX: &str = if MCP_EDITION {
    "ferrite-mcp-v"
} else {
    "ferrite-v"
};
