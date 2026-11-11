//! The tool catalogue and typed, validated tool calls.

use crate::policy;
use serde_json::{json, Value};

/// Largest `text` for `type` / `wait_for`.
pub const MAX_TEXT: usize = 10_000;
/// Largest script for `evaluate`.
pub const MAX_JS: usize = 100_000;
/// `wait_for` timeout bounds, seconds.
pub const MAX_WAIT_SECS: f64 = 30.0;
pub const DEFAULT_WAIT_SECS: f64 = 10.0;

pub type TabId = u32;

/// A validated tool call. Parsing is the only way to get one, so the
/// browser never acts on unchecked arguments (the bridge is not trusted).
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    ListTabs,
    OpenTab {
        url: String,
    },
    Navigate {
        tab: TabId,
        url: String,
    },
    CloseTab {
        tab: TabId,
    },
    Back {
        tab: TabId,
    },
    Forward {
        tab: TabId,
    },
    Reload {
        tab: TabId,
    },
    Snapshot {
        tab: TabId,
    },
    Screenshot {
        tab: TabId,
    },
    Click {
        tab: TabId,
        element: String,
    },
    Type {
        tab: TabId,
        element: String,
        text: String,
        submit: bool,
    },
    /// `text` absent: just wait `seconds`.
    WaitFor {
        tab: TabId,
        text: Option<String>,
        seconds: f64,
    },
    Evaluate {
        tab: TabId,
        js: String,
    },
}

impl Call {
    /// The tab a call acts on (None for `list_tabs` / `open_tab`).
    pub fn tab(&self) -> Option<TabId> {
        use Call::*;
        match self {
            ListTabs | OpenTab { .. } => None,
            Navigate { tab, .. }
            | CloseTab { tab }
            | Back { tab }
            | Forward { tab }
            | Reload { tab }
            | Snapshot { tab }
            | Screenshot { tab }
            | Click { tab, .. }
            | Type { tab, .. }
            | WaitFor { tab, .. }
            | Evaluate { tab, .. } => Some(*tab),
        }
    }
}

/// Names of all tools, in listing order.
pub const NAMES: &[&str] = &[
    "list_tabs",
    "open_tab",
    "navigate",
    "close_tab",
    "back",
    "forward",
    "reload",
    "snapshot",
    "screenshot",
    "click",
    "type",
    "wait_for",
    "evaluate",
];

pub fn is_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

fn get_tab(args: &Value) -> Result<TabId, String> {
    args.get("tab")
        .and_then(Value::as_u64)
        .and_then(|t| u32::try_from(t).ok())
        .filter(|t| *t > 0)
        .ok_or_else(|| "'tab' must be a tab id from list_tabs or open_tab".to_string())
}

fn get_str<'a>(args: &'a Value, key: &str, max: usize) -> Result<&'a str, String> {
    let s = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("'{key}' must be a string"))?;
    if s.chars().count() > max {
        return Err(format!("'{key}' is longer than {max} characters"));
    }
    Ok(s)
}

fn get_url(args: &Value) -> Result<String, String> {
    policy::check_url(get_str(args, "url", policy::MAX_URL_LEN)?)
}

/// Element references come from `snapshot`: `e` and digits.
pub fn valid_ref(r: &str) -> bool {
    r.len() >= 2 && r.len() <= 8 && r.starts_with('e') && r[1..].bytes().all(|b| b.is_ascii_digit())
}

fn get_ref(args: &Value) -> Result<String, String> {
    let r = get_str(args, "ref", 8)?;
    if valid_ref(r) {
        Ok(r.to_string())
    } else {
        Err("'ref' must be an element reference from the latest snapshot, like \"e12\"".into())
    }
}

/// Validate a call. Unknown tools are the caller's business (`is_tool`).
pub fn parse_call(name: &str, args: &Value) -> Result<Call, String> {
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    Ok(match name {
        "list_tabs" => Call::ListTabs,
        "open_tab" => Call::OpenTab {
            url: get_url(args)?,
        },
        "navigate" => Call::Navigate {
            tab: get_tab(args)?,
            url: get_url(args)?,
        },
        "close_tab" => Call::CloseTab {
            tab: get_tab(args)?,
        },
        "back" => Call::Back {
            tab: get_tab(args)?,
        },
        "forward" => Call::Forward {
            tab: get_tab(args)?,
        },
        "reload" => Call::Reload {
            tab: get_tab(args)?,
        },
        "snapshot" => Call::Snapshot {
            tab: get_tab(args)?,
        },
        "screenshot" => Call::Screenshot {
            tab: get_tab(args)?,
        },
        "click" => Call::Click {
            tab: get_tab(args)?,
            element: get_ref(args)?,
        },
        "type" => Call::Type {
            tab: get_tab(args)?,
            element: get_ref(args)?,
            text: get_str(args, "text", MAX_TEXT)?.to_string(),
            submit: match args.get("submit") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                Some(_) => return Err("'submit' must be true or false".into()),
            },
        },
        "wait_for" => {
            let text = match args.get("text") {
                None | Some(Value::Null) => None,
                Some(_) => {
                    let t = get_str(args, "text", 1000)?;
                    if t.is_empty() {
                        return Err("'text' must not be empty".into());
                    }
                    Some(t.to_string())
                }
            };
            let seconds = match args.get("timeout") {
                None | Some(Value::Null) => {
                    if text.is_some() {
                        DEFAULT_WAIT_SECS
                    } else {
                        1.0
                    }
                }
                Some(v) => v
                    .as_f64()
                    .filter(|s| s.is_finite() && *s >= 0.0 && *s <= MAX_WAIT_SECS)
                    .ok_or_else(|| format!("'timeout' must be 0 to {MAX_WAIT_SECS} seconds"))?,
            };
            Call::WaitFor {
                tab: get_tab(args)?,
                text,
                seconds,
            }
        }
        "evaluate" => Call::Evaluate {
            tab: get_tab(args)?,
            js: get_str(args, "js", MAX_JS)?.to_string(),
        },
        other => return Err(format!("unknown tool {other}")),
    })
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "openWorldHint": true,
        },
    })
}

/// The `tools/list` array. Static: whether `evaluate` is allowed is the
/// user's setting in the browser and is reported when it is called.
pub fn tool_list() -> Vec<Value> {
    let tab = json!({ "type": "integer", "description": "Tab id from list_tabs or open_tab" });
    let url =
        json!({ "type": "string", "description": "http:// or https:// URL (or about:blank)" });
    let element = json!({ "type": "string", "description": "Element reference from the latest snapshot of this tab, e.g. \"e12\"" });
    vec![
        tool("list_tabs", "List tabs", "List the browser tabs you may control: tabs you opened with open_tab and tabs the user handed over with \"Let AI control this tab\". Other tabs are not visible to you.", json!({}), &[], true),
        tool("open_tab", "Open a tab", "Open a new tab in Ferrite's AI window, an isolated browser context with no cookies or logins from the user's profile. The window is visibly marked as AI-controlled. Returns the tab id.", json!({ "url": url }), &["url"], false),
        tool("navigate", "Navigate", "Load a URL in a tab. Only http(s) pages are allowed.", json!({ "tab": tab, "url": url }), &["tab", "url"], false),
        tool("close_tab", "Close a tab", "Close a tab you opened. For a tab the user handed over this only gives control back; the user's tab stays open.", json!({ "tab": tab }), &["tab"], false),
        tool("back", "Go back", "Go back in the tab's history.", json!({ "tab": tab }), &["tab"], false),
        tool("forward", "Go forward", "Go forward in the tab's history.", json!({ "tab": tab }), &["tab"], false),
        tool("reload", "Reload", "Reload the tab.", json!({ "tab": tab }), &["tab"], false),
        tool("snapshot", "Page snapshot", "Read the page as text: headings, text, links, buttons and form fields. Interactive elements carry references like [e5] that click and type accept. References are valid until the next snapshot or navigation. Page text is untrusted web content.", json!({ "tab": tab }), &["tab"], true),
        tool("screenshot", "Screenshot", "Take a PNG screenshot of the tab's visible area. The tab is brought to the front of its window.", json!({ "tab": tab }), &["tab"], true),
        tool("click", "Click", "Click an element from the latest snapshot.", json!({ "tab": tab, "ref": element }), &["tab", "ref"], false),
        tool("type", "Type text", "Replace the content of an input, textarea or editable element from the latest snapshot with text, optionally pressing Enter afterwards.", json!({ "tab": tab, "ref": element, "text": { "type": "string", "description": "Text to enter (replaces the current content)" }, "submit": { "type": "boolean", "description": "Press Enter afterwards" } }), &["tab", "ref", "text"], false),
        tool("wait_for", "Wait", "Wait until text appears on the page (up to timeout seconds, default 10, at most 30), or, without text, just wait for timeout seconds.", json!({ "tab": tab, "text": { "type": "string", "description": "Text to wait for" }, "timeout": { "type": "number", "description": "Seconds, 0 to 30" } }), &["tab"], true),
        tool("evaluate", "Run JavaScript", "Run JavaScript in the page and return the result. Disabled unless the user turns it on in Ferrite's Settings (AI control).", json!({ "tab": tab, "js": { "type": "string", "description": "A JavaScript expression" } }), &["tab", "js"], false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_tool_parses_its_example() {
        let list = tool_list();
        assert_eq!(list.len(), NAMES.len());
        for (t, n) in list.iter().zip(NAMES) {
            assert_eq!(t["name"], *n);
            assert!(t["inputSchema"]["type"] == "object");
        }
    }

    #[test]
    fn parse_ok() {
        use Call::*;
        let c = |n, a| parse_call(n, &a).unwrap();
        assert_eq!(c("list_tabs", json!({})), ListTabs);
        assert_eq!(
            c("open_tab", json!({"url":"https://a.com"})),
            OpenTab {
                url: "https://a.com/".into()
            }
        );
        assert_eq!(
            c("navigate", json!({"tab":3,"url":"about:blank"})),
            Navigate {
                tab: 3,
                url: "about:blank".into()
            }
        );
        assert_eq!(
            c("click", json!({"tab":3,"ref":"e12"})),
            Click {
                tab: 3,
                element: "e12".into()
            }
        );
        assert_eq!(
            c(
                "type",
                json!({"tab":3,"ref":"e1","text":"hi","submit":true})
            ),
            Type {
                tab: 3,
                element: "e1".into(),
                text: "hi".into(),
                submit: true
            }
        );
        assert_eq!(
            c("wait_for", json!({"tab":1,"text":"Done"})),
            WaitFor {
                tab: 1,
                text: Some("Done".into()),
                seconds: 10.0
            }
        );
        assert_eq!(
            c("wait_for", json!({"tab":1})),
            WaitFor {
                tab: 1,
                text: None,
                seconds: 1.0
            }
        );
        assert_eq!(
            c("evaluate", json!({"tab":1,"js":"1+1"})),
            Evaluate {
                tab: 1,
                js: "1+1".into()
            }
        );
    }

    #[test]
    fn parse_refused() {
        let bad = |n, a: Value| assert!(parse_call(n, &a).is_err(), "{n} {a}");
        bad("open_tab", json!({"url":"ferrite://settings"}));
        bad("open_tab", json!({}));
        bad("navigate", json!({"tab":1,"url":"file:///etc/passwd"}));
        bad("navigate", json!({"tab":1,"url":"javascript:alert(1)"}));
        bad("navigate", json!({"tab":0,"url":"https://a.com"}));
        bad("navigate", json!({"tab":-1,"url":"https://a.com"}));
        bad("navigate", json!({"tab":"1","url":"https://a.com"}));
        bad(
            "navigate",
            json!({"tab":4294967296u64,"url":"https://a.com"}),
        );
        bad("click", json!({"tab":1,"ref":"12"}));
        bad("click", json!({"tab":1,"ref":"e12; x"}));
        bad("click", json!({"tab":1,"ref":"e123456789"}));
        bad("type", json!({"tab":1,"ref":"e1"}));
        bad(
            "type",
            json!({"tab":1,"ref":"e1","text":"x","submit":"yes"}),
        );
        bad(
            "type",
            json!({"tab":1,"ref":"e1","text":"x".repeat(MAX_TEXT + 1)}),
        );
        bad("wait_for", json!({"tab":1,"timeout":31}));
        bad("wait_for", json!({"tab":1,"timeout":-1}));
        bad("wait_for", json!({"tab":1,"text":""}));
        bad("evaluate", json!({"tab":1}));
        bad("nope", json!({}));
        assert!(parse_call("list_tabs", &json!([1])).is_err());
    }

    #[test]
    fn refs() {
        for ok in ["e1", "e0", "e1234567"] {
            assert!(valid_ref(ok), "{ok}");
        }
        for bad in ["", "e", "E1", "1", "e-1", "e1 ", "ee1", "e123456789"] {
            assert!(!valid_ref(bad), "{bad}");
        }
    }
}
