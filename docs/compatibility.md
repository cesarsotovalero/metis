# Compatibility

Recorded 2026-11-05. Toolchain: rustc 1.99.1 (58cf8f9c3 2026-10-13), Cargo from the same stable distribution. Rust 1.99.1 is the minimum tested version, not a claim that older releases fail. Edition 2021; exact dependency resolution is in Cargo.lock. Bundled SQLite is pinned transitively by the lockfile. Dependency/license audit is not yet performed.

| Combination | Status |
|---|---|
| Linux x86_64 fixture import/JSON/Markdown | tested offline |
| macOS, WSL2, Linux aarch64 | not tested |
| Native Windows | not tested; no launcher |
| Claude Code 2.1.318 installed | recorded only; no live adapter |
| Codex CLI 0.166.0 installed | recorded only; no live adapter |
| Fixture schema v1 | supported subset: lifecycle, request/tool state, usage, visibility gaps |
| Other versions/kinds/extensions | rejected atomically |
| Daemon, TUI, launcher, hooks, recovery, optimizer | unavailable |

No upstream integration is advertised; no live upstream wire fixtures have been validated. Usage schema is canonical synthetic input: total input includes cache categories; reasoning inclusion is explicit. Cumulative usage is a full authoritative revision, including absent fields. Deltas are additive for present categories. Producers must not mix cumulative and delta streams for the same request without specifying a valid combined sequence. Multiple producers/epochs sharing a call identity are rejected; supporting them requires a future reconciliation adapter.

OS data directory behavior is currently Linux/XDG only; use explicit --home on other platforms. The CLI is synchronous and bounded by a file-size cap, not a production high-volume ingestion service.
