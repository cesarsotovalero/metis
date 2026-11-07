# Local daemon IPC implementation plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete O01's authenticated, bounded local observer IPC and its writer foundation.
**Architecture:** A private Linux Unix socket, bounded connection workers and one bounded SQLite writer actor. Existing event schema and storage remain authoritative.
**Tech Stack:** Rust 1.99.1, standard-library threads/socket/file locks, nix peer credentials, getrandom, existing Serde and SQLite.
**Spec:** docs/superpowers/specs/2026-11-07-daemon-ipc-design.md and docs/specification.md sections 6–7, ticket O01.

## Global constraints

- First-party code forbids unsafe; all application logic is Rust.
- Observe and synthetic metadata only; no agent/config/service installation or paid calls.
- 8 MiB frame cap; transactional append; unknown versions/capabilities fail safely.
- Preserve existing uncommitted feature and rename work. Do not commit or publish it as a side effect.

## Review focus

- A slow sender must not renew its frame deadline indefinitely.
- Another server/startup failure must not remove active runtime resources.
- A mismatched producer/session in a batch must commit no prefix.
- A timed-out queued write must not execute later; an executing write has an explicit uncertain outcome.
- Queue/connection overload must stay bounded and observable, including spool failures.

## Task 1: protocol/runtime boundaries

Files: new crates/cs-daemon/{Cargo.toml,src/lib.rs,src/protocol.rs,src/runtime.rs}; workspace manifest/lock.
Interfaces: Config with frame/connection/queue/batch/deadline bounds; Request/Operation/Response/ResultPayload/SafeError; framed read/write; private Runtime guard with UID and token validation.
- [x] Write subprocess acceptance test for start/status/stop and observe failure against the absent command.
- [x] Add typed framing, exact deadlines, private runtime lock/token/socket handling and Linux peer verification.
- [x] Add/run real-socket framing/permission/singleton rejection tests; observe meaningful rejection.
Expected: boundaries refuse malformed/oversized/unauthenticated input without persisting it.

## Task 2: writer and authenticated server

Files: new crates/cs-daemon/src/{writer.rs,server.rs,client.rs}, crate tests; cs-store bounded writer lock configuration.
Interfaces: `run(home: &Path, config: Config) -> Result<()>`; authenticated `Client::connect(home, binding)` and request; single-owner writer with sync-channel admission and per-request expiry.
- [x] Write tests for missing authentication, wrong version/session, atomic batch/dedup and snapshots.
- [x] Implement negotiation and only the supported operations, health counters, bounded loss spool and queued-write expiry.
- [x] Test overload with real held SQLite transaction; test frame/dispatch timeouts, permissions, duplicate server, restart and cleanup.
Expected: only authorized session-bound metadata commits; bounds never silently lose admitted work.

## Task 3: CLI, evidence and review

Files: crates/mertis/src/main.rs, Cargo.toml and tests/daemon.rs; docs/{status,compatibility,README/daemon} and ADR.
- [x] Wire daemon start/status/stop before offline Store construction.
- [x] Run subprocess and whole-workspace tests, formatting, Clippy and build.
- [x] Run an isolated demonstration; document measured status, timeout semantics and remaining M0/M1 gaps.
- [x] Obtain a read-only fresh review, fix substantive findings with regression tests, then verify final checks.
Expected: foreground daemon and control commands work offline; no claim that all of M0 or M1 is complete.

## Execution ledger

Ruling: user authorized roadmap implementation and execution in the existing project; the specification already determines the transport and authentication boundary. Use the concrete scope above without reopening the existing design for approval. Work in the current dirty checkout to preserve and build on its uncommitted contract/rename changes; no commits, pushes or PR changes are authorized by this turn.

Task 1: complete — first subprocess test failed because daemon start was absent; protocol/runtime and real-socket tests now pass. Private endpoints, version/authentication rejection and absolute frame deadlines verified.
Task 2: complete — writer/session tests pass, including real locked-database admission, schema/accounting batch rollback and restart. Independent review corrections: stalled loss reporting and duplicate queued-expiry counting both reproduced RED then passed GREEN. Spool admission/drain is bounded, with observable overflow/failure.
Task 3: complete — CLI lifecycle, SIGTERM and explicit crash restart tests pass. Whole suite: 44 tests. Format/build/Clippy checks pass. Example sends cached fixture through IPC and reads its snapshot. Existing implementation and rename work remain uncommitted and were not published.
Ruling: standard-library bounded workers fit the current synchronous codebase; no Tokio dependency is introduced. Cost if wrong: an async integration may later require replacing scheduling behind the same protocol.
Ruling: loss spool contains bounded metadata-only notices, not replayable events, consistent with current metadata/privacy boundary. Cost if wrong: producers that discard unacknowledged originals cannot recover them from this diagnostic spool.
Ruling: owner token + same-UID peer authentication binds each connection, not mutually hostile processes under one owner. Cost if wrong: same-owner credential compromise can open additional session bindings.
Final review: independent read-only reviewer completed. Important stalled-spool finding and expiry-count finding fixed with reproducing tests. Reviewed exclusions (live adapters, TUI, transforms, exact artifact recovery/replay, non-Linux support, broader M0/M1 completion) remain future tickets; no capability or completion claim is made for them.
Final regression: deferred transaction upgrade bypassed the configured SQLite busy timeout; `writer_lock_contention_obeys_the_configured_busy_timeout` reproduced RED and passed GREEN after acquiring an immediate write reservation.

Demonstration evidence: protocol 1, observe mode, cached session 7 events / 150 input tokens, repeat deduplicated, clean stop removed endpoints. Final checks: 44/44 tests; format, workspace build, example build and all-target Clippy pass. All prior work preserved; no new commit, push or PR changes.
