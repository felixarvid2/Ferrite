//! `ferrite-mcp`: lets an MCP client (Claude Desktop, Claude Code, ...)
//! control Ferrite. The client launches this program and speaks MCP on its
//! stdin/stdout; each tool call is forwarded to the running browser over a
//! Unix domain socket in the profile directory (no network listener).
//!
//! `initialize`, `server/discover` and `tools/list` are answered here even
//! when Ferrite is not running, so the client does not mark the server as
//! failed; a tool call then returns a readable error.

use ferrite_mcp::rpc::{Backend, Server, ToolOutput};
use ferrite_mcp::tools::Call;
use ferrite_mcp::wire::{self, Line};
use serde_json::{json, Value};
use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const HELP: &str = "\
Usage: ferrite-mcp [--profile=NAME] [--socket=PATH]

MCP server on stdin/stdout for controlling the Ferrite browser. Enable
\"AI control (MCP)\" in Ferrite's Settings first. The socket defaults to
<data dir>/profiles/NAME/mcp.sock (NAME defaults to \"default\").
Environment: FERRITE_DATA_DIR, FERRITE_MCP_SOCKET.";

/// A tool call may legitimately take this long (wait_for is capped at 30 s).
const CALL_TIMEOUT: Duration = Duration::from_secs(90);

struct SocketBackend {
    path: PathBuf,
    next: AtomicU64,
}

impl Backend for SocketBackend {
    fn call(&self, call: Call) -> ToolOutput {
        let (tool, args) = to_wire(&call);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut stream = match UnixStream::connect(&self.path) {
            Ok(s) => s,
            Err(_) => {
                return ToolOutput::error(
                    "Cannot reach Ferrite. Start Ferrite and turn on \"AI control (MCP)\" in its Settings, \
                     then try again.",
                )
            }
        };
        let _ = stream.set_read_timeout(Some(CALL_TIMEOUT));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        let line = wire::request_line(id, tool, &args);
        if line.len() > wire::MAX_REQUEST_LINE {
            return ToolOutput::error("Request too large");
        }
        if writeln!(stream, "{line}")
            .and_then(|_| stream.flush())
            .is_err()
        {
            return ToolOutput::error("Lost the connection to Ferrite");
        }
        let mut reader = BufReader::new(stream);
        let reply = match wire::read_line_bounded(&mut reader, wire::MAX_RESPONSE_LINE) {
            Ok(Line::Text(t)) => t,
            Ok(Line::TooLong) => return ToolOutput::error("Ferrite's answer was too large"),
            Ok(Line::Eof) | Err(_) => {
                return ToolOutput::error(
                    "Ferrite did not answer (it may have closed or the tool timed out)",
                )
            }
        };
        let Ok(v) = serde_json::from_str::<Value>(&reply) else {
            return ToolOutput::error("Ferrite sent an unreadable answer");
        };
        if v.get("id").and_then(Value::as_u64) != Some(id) {
            return ToolOutput::error("Ferrite sent an unexpected answer");
        }
        match v.get("content").and_then(Value::as_array) {
            Some(content) if v.get("error").is_none_or(Value::is_null) => ToolOutput {
                content: content.clone(),
                is_error: v.get("ok").and_then(Value::as_bool) != Some(true),
            },
            _ => ToolOutput::error(
                v.get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Ferrite refused the request"),
            ),
        }
    }
}

/// The tool name and arguments of a validated call, as sent on the wire.
fn to_wire(call: &Call) -> (&'static str, Value) {
    match call {
        Call::ListTabs => ("list_tabs", json!({})),
        Call::OpenTab { url } => ("open_tab", json!({ "url": url })),
        Call::Navigate { tab, url } => ("navigate", json!({ "tab": tab, "url": url })),
        Call::CloseTab { tab } => ("close_tab", json!({ "tab": tab })),
        Call::Back { tab } => ("back", json!({ "tab": tab })),
        Call::Forward { tab } => ("forward", json!({ "tab": tab })),
        Call::Reload { tab } => ("reload", json!({ "tab": tab })),
        Call::Snapshot { tab } => ("snapshot", json!({ "tab": tab })),
        Call::Screenshot { tab } => ("screenshot", json!({ "tab": tab })),
        Call::Click { tab, element } => ("click", json!({ "tab": tab, "ref": element })),
        Call::Type {
            tab,
            element,
            text,
            submit,
        } => (
            "type",
            json!({ "tab": tab, "ref": element, "text": text, "submit": submit }),
        ),
        Call::WaitFor { tab, text, seconds } => (
            "wait_for",
            json!({ "tab": tab, "text": text, "timeout": seconds }),
        ),
        Call::Evaluate { tab, js } => ("evaluate", json!({ "tab": tab, "js": js })),
    }
}

fn main() {
    let mut profile = "default".to_string();
    let mut socket: Option<PathBuf> = None;
    for arg in std::env::args().skip(1) {
        if let Some(p) = arg.strip_prefix("--profile=") {
            profile = p.to_string();
        } else if let Some(p) = arg.strip_prefix("--socket=") {
            socket = Some(PathBuf::from(p));
        } else if arg == "-h" || arg == "--help" {
            println!("{HELP}");
            return;
        } else {
            eprintln!("ferrite-mcp: unknown argument {arg:?}\n{HELP}");
            std::process::exit(2);
        }
    }
    let valid = !profile.is_empty()
        && profile.len() <= 64
        && profile
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        eprintln!("ferrite-mcp: invalid profile name");
        std::process::exit(2);
    }
    let Some(path) = socket.or_else(|| wire::socket_path(&profile)) else {
        eprintln!("ferrite-mcp: cannot work out the profile directory; pass --socket=PATH");
        std::process::exit(2);
    };
    let server = Server::new(SocketBackend {
        path,
        next: AtomicU64::new(1),
    });
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    loop {
        let line = match wire::read_line_bounded(&mut input, wire::MAX_REQUEST_LINE) {
            Ok(Line::Eof) | Err(_) => return,
            Ok(Line::TooLong) => {
                // Cannot know the id of an oversize message.
                let e = json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"message too large"}});
                let mut out = stdout.lock();
                let _ = writeln!(out, "{e}");
                let _ = out.flush();
                continue;
            }
            Ok(Line::Text(t)) => t,
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_line(&line) {
            let mut out = stdout.lock();
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                return; // The client went away.
            }
        }
    }
}
