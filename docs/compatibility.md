# Compatibility

Recorded 2026-11-07. Toolchain: rustc 1.99.1 (58cf8f9c3 2026-10-13), Cargo from the same stable distribution. Rust 1.99.1 is the minimum tested version, not a claim that older releases fail. Edition 2021; exact dependency resolution is in Cargo.lock. Bundled SQLite is pinned transitively by the lockfile. Dependency/license metadata and the new validator review are in [dependencies](dependencies.md); full advisory/source/packaging audits remain.

| Combination | Status |
|---|---|
| Linux x86_64 fixture import/JSON/Markdown | tested offline |
| macOS, WSL2, Linux aarch64 | not tested |
| Native Windows | not tested; no launcher |
| Claude Code 2.1.318 installed | recorded only; no live adapter |
| Codex CLI 0.166.0 installed | recorded only; no live adapter |
| Fixture schema v1 | all 35 typed metadata event variants; fixture conformance tested |
| Database schemas 1/2 -> 3 | transactional upgrade/backfill tested; newer schemas refused |
| Other versions/kinds/extensions | rejected atomically |
| Linux x86_64 daemon IPC v1 | authenticated synthetic append/snapshot/health/shutdown tested |
| Other daemon platforms | explicitly unsupported |
| TUI, launcher, live adapters, hooks, recovery, optimizer | unavailable |

No upstream integration is advertised; no live upstream wire fixtures have been validated. Usage schema is canonical synthetic input: total input includes cache categories; reasoning inclusion is explicit. Cumulative usage is a full authoritative revision, including absent fields. Deltas are additive for present categories. Producers must not mix cumulative and delta streams for the same request without specifying a valid combined sequence. Multiple producers/epochs sharing a call identity are rejected; supporting them requires a future reconciliation adapter.

OS data directory behavior is currently Linux/XDG only; use explicit --home on other platforms. Offline import remains synchronous and bounded by file/record caps. The daemon uses a dedicated writer and bounded standard-library socket workers; no production throughput target is claimed.

Resume linkage is supported only for an explicit linear chain within one producer. Missing parent epochs can reconcile after late import. Branches, cycles, repeated markers, missing within-epoch ordering, and multiple lifecycle producers remain unknown. Shared call identities across epochs still fail atomically. Task outcomes are source assertions with evidence kind and optional oracle version; recording a passed assertion does not run an oracle. Cancellation is separate from failure; contradictory terminal states are unknown (included in incomplete counts and also counted separately).

Advanced evidence/context/recovery/policy events are persisted and counted only. They grant no artifact access, delivery confirmation, verified adapter functionality, or capture-mode change. Import remains synthetic metadata/observe. Artifact and policy identifiers are references, not proof that bytes or services exist.

The embedded draft-2020-12 JSON Schema is executed offline by jsonschema 0.60.2 with default features disabled. Unknown fields, including on empty lifecycle payloads and nested measurements, fail before persistence. Opaque metadata strings are bounded to 128 Unicode characters without controls; file and record caps are enforced before unbounded allocation. No extensible arbitrary JSON fields are accepted in this wire version.

M1 O01 details and exact limits are in [daemon.md](daemon.md). Linux peer UID and a private owner token authenticate connections; session/producer/epoch bindings constrain each connection. Negative token/version/session tests pass; a separate-UID operating-system rejection test has not been run. The runtime directory refuses unsafe ownership, permissions and symlink endpoints. SIGTERM shutdown and stale endpoint recovery after a forced crash are tested. Original source records remain immutable.
