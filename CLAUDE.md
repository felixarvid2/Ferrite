# Working in this repository

Ferrite: a desktop web browser in Rust on CEF/Chromium (the Cargo workspace at the root:
`app/`, `core/`, `ui/`) plus Ferrite for Android (`android/`, a WebView app with the same
Rust ad blocker in `android/rust`). Start with `README.md`, `docs/STATUS.md` (what is verified,
known limitations) and `docs/ARCHITECTURE.md`; Android details are in `android/README.md`.

Project skills in `.claude/skills/`: `ferrite-cef` (CEF/threads/panic rules), `ferrite-security`
(threat model and review checklist), `ferrite-e2e` (driving the real browser under Xvfb).

CI: `.github/workflows/ferrite-ci.yml` (desktop fmt/clippy/tests), `ferrite-android.yml`
(Android build, tests, emulator smoke test; `publish` input releases the APK) and
`ferrite-release.yml` (`.deb` + AppImage; `publish` input creates the `ferrite-v<version>` release).
A private H.264 build lives in felixarvid2/eget-repo (`ferrite-h264.yml`).

## Who does what

Work is split by model; the subagents live in `.claude/agents/`.

| Work | Who | Model |
|---|---|---|
| Finding files and symbols, reading code, answering "where/how" | `Explore` | Haiku |
| Simple mechanical edits that are already fully specified (renames, strings, data values, the same small change in many places) | `mechanic` | Haiku |
| Regular coding: implementing a specified feature or fix, tests, planned refactors | `implementer` | Sonnet |
| Running a project's checks (tests, build, lint) and reporting only the verdict and failures | `verifier` | Haiku |
| Docs from a short summary: READMEs, notes, commit and PR bodies | `scribe` | Haiku |
| Shipping: commit, push, PR (merge only after the user approves) | `shipper` | Haiku |
| Architecture, trade-offs, hard bugs with an unknown cause, risk review | the main session, or `architect` when the main session runs a smaller model | Opus |

- Delegate search and reading to `Explore` instead of reading many files yourself, and pass it the
  question, not a file list to dump.
- Hand `implementer` a clear spec (what, where, how to verify); hand `mechanic` the exact change.
  Run independent pieces in parallel.
- Keep decisions here: choosing an approach, reviewing what subagents return, and anything that
  spans several systems. Check their results (the diff and the verifier's verdict) before committing.
- Hand `verifier` the project to check; hand `scribe` the facts. `shipper` merges a pull request
  only when told that the user approved that merge.
- Standing approval from the user: after each change, once the checks pass, merge its pull request
  into `main` (merge commit) without asking first.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. When the answer is under a Markdown heading, `graphify section "<heading>"` prints that section's text, so the .md need not be opened. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- A committed graphify-out/graph.json is rewritten whole on every build, so a large diff to it in a pull request is expected. Do not flag it as churn or suggest skipping the graph rebuild or its hooks.
