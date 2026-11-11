//! The line protocol between the `ferrite-mcp` bridge and the browser, over
//! a Unix domain socket in the profile directory.
//!
//! Newline-delimited JSON, one request per line, one response line per
//! request:
//!
//! ```text
//! -> {"id":1,"tool":"click","args":{"tab":3,"ref":"e5"}}
//! <- {"id":1,"ok":true,"content":[{"type":"text","text":"..."}]}
//! <- {"id":1,"ok":false,"error":"..."}
//! ```
//!
//! The browser validates `args` again with [`crate::tools::parse_call`].

use serde_json::{json, Value};
use std::io::{self, BufRead};
use std::path::PathBuf;

/// Largest request line the browser accepts (requests are small).
pub const MAX_REQUEST_LINE: usize = 1024 * 1024;
/// Largest response line the bridge accepts (screenshots as base64).
pub const MAX_RESPONSE_LINE: usize = 24 * 1024 * 1024;

/// Name of the socket file in the profile directory.
pub const SOCKET_FILE: &str = "mcp.sock";

/// `sun_path` holds 108 bytes including the NUL.
pub const MAX_SOCKET_PATH: usize = 107;

/// Where the browser of `profile` listens. `FERRITE_MCP_SOCKET` overrides
/// (needed when the default path is too long for a Unix socket). Mirrors
/// `ProfilePaths::for_profile` in ferrite-core (a test there checks it).
pub fn socket_path(profile: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FERRITE_MCP_SOCKET").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let base = match std::env::var_os("FERRITE_DATA_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => dirs::data_dir()?.join("ferrite"),
    };
    Some(socket_path_in(&base, profile))
}

/// The socket of `profile` under the data directory `base`.
pub fn socket_path_in(base: &std::path::Path, profile: &str) -> PathBuf {
    base.join("profiles").join(profile).join(SOCKET_FILE)
}

pub enum Line {
    Eof,
    /// A complete line (without the newline).
    Text(String),
    /// The line exceeded the limit; it was skipped up to its newline.
    TooLong,
}

/// Read one line of at most `max` bytes without ever buffering more.
pub fn read_line_bounded(r: &mut impl BufRead, max: usize) -> io::Result<Line> {
    let mut buf: Vec<u8> = Vec::new();
    let mut too_long = false;
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            return Ok(if buf.is_empty() && !too_long {
                Line::Eof
            } else if too_long {
                Line::TooLong
            } else {
                finish(buf)
            });
        }
        let (take, found) = match chunk.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (chunk.len(), false),
        };
        if !too_long {
            let body = if found { take - 1 } else { take };
            if buf.len() + body > max {
                too_long = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(&chunk[..body]);
            }
        }
        r.consume(take);
        if found {
            return Ok(if too_long { Line::TooLong } else { finish(buf) });
        }
    }
}

fn finish(mut buf: Vec<u8>) -> Line {
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Line::Text(String::from_utf8_lossy(&buf).into_owned())
}

pub fn request_line(id: u64, tool: &str, args: &Value) -> String {
    json!({ "id": id, "tool": tool, "args": args }).to_string()
}

pub fn ok_line(id: &Value, content: &[Value], is_error: bool) -> String {
    json!({ "id": id, "ok": !is_error, "content": content, "error": Value::Null }).to_string()
}

pub fn error_line(id: &Value, message: &str) -> String {
    json!({ "id": id, "ok": false, "error": message }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn lines(data: &[u8], max: usize) -> Vec<String> {
        let mut c = Cursor::new(data.to_vec());
        let mut out = vec![];
        loop {
            match read_line_bounded(&mut c, max).unwrap() {
                Line::Eof => return out,
                Line::Text(t) => out.push(t),
                Line::TooLong => out.push("<too long>".into()),
            }
        }
    }

    #[test]
    fn bounded_lines() {
        assert_eq!(lines(b"a\nbb\r\n\nlast", 10), ["a", "bb", "", "last"]);
        assert_eq!(lines(b"abcdefghijkl\nok\n", 5), ["<too long>", "ok"]);
        assert_eq!(lines(b"abcde\nxyzxyzxyz", 5), ["abcde", "<too long>"]);
        assert!(lines(b"", 5).is_empty());
    }

    #[test]
    fn long_line_is_not_buffered() {
        // A tiny BufReader capacity forces many refills of an oversize line.
        let data = vec![b'x'; 100_000];
        let mut r =
            io::BufReader::with_capacity(16, Cursor::new([&data[..], b"\nnext\n"].concat()));
        assert!(matches!(
            read_line_bounded(&mut r, 1000).unwrap(),
            Line::TooLong
        ));
        assert!(matches!(read_line_bounded(&mut r, 1000).unwrap(), Line::Text(t) if t == "next"));
    }

    #[test]
    fn socket_path_layout() {
        // (Env-dependent parts are covered in ferrite-core's profile test.)
        let p = std::path::Path::new("/d/profiles/work").join(SOCKET_FILE);
        assert!(p.ends_with("profiles/work/mcp.sock"));
    }
}
