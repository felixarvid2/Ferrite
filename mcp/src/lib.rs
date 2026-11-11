//! MCP support shared by the `ferrite-mcp` bridge binary and the browser.
//!
//! - [`rpc`]: JSON-RPC 2.0 / MCP message handling (both protocol eras).
//! - [`tools`]: the tool catalogue and typed, validated tool calls.
//! - [`policy`]: URL allowlist, who may control a tab, untrusted-content marking.
//! - [`wire`]: the line protocol between the bridge and the browser, and
//!   bounded line reading.
//!
//! No async runtime and no CEF here: everything is plain `std` + `serde_json`
//! so it can be unit tested and the bridge stays a small binary.

pub mod policy;
pub mod rpc;
pub mod tools;
pub mod wire;
