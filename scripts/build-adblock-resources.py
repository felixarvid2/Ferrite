#!/usr/bin/env python3
"""Convert uBlock Origin's scriptlets into adblock-rust resources.

uBlock Origin (GPL-3.0) defines its scriptlets in src/js/resources/*.js as

    registerScriptlet(jsonPrune, {
        name: 'json-prune.js',
        dependencies: [ objectPruneFn, safeSelf ],   (functions or names)
    });

or, in older parts, as

    builtinScriptlets.push({
        name: 'json-prune.js',
        aliases: [ 'json-prune' ],
        requiresTrust: true,          (optional)
        fn: jsonPrune,
        dependencies: [ 'safe-self.fn' ],
    });
    function jsonPrune(...) {
    ...
    }

Brave converts them by importing the module in Node and calling
fn.toString(). This script reads the same data statically, without running
any of uBlock's code: top-level functions and classes in uBlock's sources
start at column 0 with `function name(` or `class Name {` (optionally
`export`/`async`) and end at the next line that is exactly `}`.

Output: the JSON array Engine::use_resources reads (name, aliases, kind,
content as base64, dependencies, permission). Trusted scriptlets get
permission bit 0, so only lists loaded with that permission can use them.

    python3 -I scripts/build-adblock-resources.py <uBlock checkout> <out.json>
"""
import base64
import json
import re
import sys
from pathlib import Path

PUSH = re.compile(r"^builtinScriptlets\.push\(\{\n(.*?)^\}\);", re.S | re.M)
REGISTER = re.compile(r"^registerScriptlet\(([A-Za-z0-9_$]+),\s*\{\n(.*?)^\}\);", re.S | re.M)
FUNC = re.compile(r"^(?:export\s+)?((?:async\s+)?function\s*\*?\s*([A-Za-z0-9_$]+)\s*\(|class\s+([A-Za-z0-9_$]+)\s*\{)", re.M)
# Files outside resources/ that scriptlets import (classes and helpers).
SHARED = ["urlskip.js", "jsonpath.js", "arglist-parser.js"]


def list_items(body, key):
    """Items of `key: [ ... ]`: quoted names or bare function identifiers."""
    m = re.search(rf"^\s*{key}:\s*\[(.*?)\]", body, re.S | re.M)
    if not m:
        return []
    return [i.strip().strip("'") for i in m.group(1).split(",") if i.strip()]


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: build-adblock-resources.py <uBlock checkout> <out.json>")
    src = Path(sys.argv[1]) / "src/js/resources"
    texts = [p.read_text(encoding="utf-8") for p in sorted(src.glob("*.js"))]
    shared = [(src.parent / f).read_text(encoding="utf-8") for f in SHARED]

    functions = {}
    for text in texts + shared:
        for m in FUNC.finditer(text):
            end = text.find("\n}\n", m.start())
            if end < 0:
                continue
            ident = m.group(2) or m.group(3)
            if ident in functions:
                sys.exit(f"duplicate top-level definition: {ident}")
            functions[ident] = m.group(1) + text[m.end():end + 2]

    entries = []  # (function identifier, details body)
    for text in texts:
        for m in PUSH.finditer(text):
            fn = re.search(r"^\s*fn:\s*([A-Za-z0-9_$]+)", m.group(1), re.M)
            if not fn:
                sys.exit(f"cannot parse entry:\n{m.group(1)}")
            entries.append((fn.group(1), m.group(1)))
        for m in REGISTER.finditer(text):
            entries.append((m.group(1), m.group(2)))

    name_of = {}  # function identifier -> scriptlet name
    for fn, body in entries:
        name = re.search(r"^\s*name:\s*'([^']+)'", body, re.M)
        if not name:
            sys.exit(f"cannot parse entry:\n{body}")
        name_of[fn] = name.group(1)

    resources = []
    for fn, body in entries:
        if fn not in functions:
            sys.exit(f"{name_of[fn]}: function {fn} not found")
        resources.append({
            "name": name_of[fn],
            "aliases": list_items(body, "aliases"),
            "kind": {"mime": "application/javascript"},
            "content": base64.b64encode(functions[fn].encode()).decode(),
            "dependencies": [name_of.get(d, d) for d in list_items(body, "dependencies")],
            "permission": 1 if re.search(r"^\s*requiresTrust:\s*true", body, re.M) else 0,
        })

    # adblock-rust passes filter arguments only to scriptlets whose content
    # starts with `function name(`; dependencies (*.fn) may be anything.
    for r in resources:
        content = base64.b64decode(r["content"]).decode()
        if r["name"].endswith(".js") and not content.startswith("function "):
            sys.exit(f"{r['name']}: scriptlet is not a plain function")

    names = {r["name"] for r in resources}
    missing = [(r["name"], d) for r in resources for d in r["dependencies"] if d not in names]
    if missing:
        sys.exit(f"missing dependencies: {missing}")
    if len(resources) < 50:
        sys.exit(f"only {len(resources)} scriptlets found; uBlock's layout changed?")
    Path(sys.argv[2]).write_text(json.dumps(resources, indent=0) + "\n", encoding="utf-8")
    print(f"{len(resources)} scriptlets -> {sys.argv[2]}")


if __name__ == "__main__":
    main()
