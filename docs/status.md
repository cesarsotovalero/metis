# Implementation status — 2026-11-07

Implemented the **section 25 first assignment**, the **M0 F02 event-contract follow-up**, and **M1 O01 Linux daemon IPC**, with scoped deviations recorded below. This is an offline foundation, not completion of M0–M8 or an R0 release. No live integration, optimization, model calls, global agent configuration, or deployment occurred.

| Requirement | Implementation | Tests / fixture | Demonstration |
|---|---|---|---|
| Minimal four-crate workspace, pinned toolchain | [workspace](../Cargo.toml), [toolchain](../rust-toolchain.toml), [lock](../Cargo.lock), [CI](../.github/workflows/ci.yml) | full workspace checks | `cargo build --workspace --locked` |
| Typed v1 event envelope, explicit measurement provenance | [core](../crates/cs-core/src/lib.rs), [wire schema](../schemas/event-v1.json) | `wire_round_trip_and_strict_privacy_boundary`; all JSONL fixtures | import any fixture below |
| Cache/reasoning accounting, cumulative revisions, deltas, missingness, overflow | [accounting](../crates/cs-accounting/src/lib.rs) | four accounting tests; [cached](../fixtures/cached.jsonl) | `mertis import --agent fixture --path fixtures/cached.jsonl` then `mertis inspect cached --json` |
| SQLite WAL, foreign keys, numbered migration, immutable events, materialized calls | [store](../crates/cs-store/src/lib.rs), [migration](../crates/cs-store/migrations/001.sql) | `migration_integrity_immutability_and_materialized_records`, `restart_and_newer_schema` | reimport the same trace after restarting CLI |
| Atomic import, source deduplication, late events, conflicts | [store](../crates/cs-store/src/lib.rs) | `late_events_reconcile_and_source_duplicates_dedup`, `malformed_rollback_and_conflict`; [interrupted](../fixtures/interrupted.jsonl) | import interrupted twice; inspect interrupted |
| 100-turn offline totals and synthetic labeling | [ordinary fixture](../fixtures/ordinary.jsonl), [CLI](../crates/mertis/src/main.rs) | `dedup_and_100_turn_totals`, [CLI subprocess test](../crates/mertis/tests/cli.rs) | README commands |
| JSON and Markdown report, provenance and unavailable cost | [report](../crates/cs-core/src/lib.rs), [sample](sample-session.md) | CLI subprocess test and store accounting assertions | `mertis inspect ordinary --json` or `mertis inspect ordinary` |
| Session lifecycle reconciliation (M0 F02/F03/F04 follow-up) | [report types](../crates/cs-core/src/lib.rs), [store](../crates/cs-store/src/lib.rs), [migration 002](../crates/cs-store/migrations/002.sql) | `lifecycle_late_events_missing_markers_and_ambiguity`, `conflicting_lifecycle_markers_do_not_claim_an_end`, `additive_lifecycle_migration_preserves_v1_events`; [lifecycle fixture](../fixtures/lifecycle.jsonl); CLI subprocess assertions | `mertis --home /tmp/mertis-lifecycle-demo import --agent fixture --path fixtures/lifecycle.jsonl` then `mertis --home /tmp/mertis-lifecycle-demo inspect lifecycle --json` |

Use `target/debug/mertis` after building, or the `cargo run --locked -p mertis --` prefix from [README](../README.md). All demonstration imports should specify `--home /tmp/mertis-demo` before the subcommand, or set `MERTIS_HOME`, to isolate test data.

## Validation

Environment: Linux x86_64. Installed isolated Rust under `/tmp/contextscope-cargo` and `/tmp/contextscope-rustup`; no existing Rust toolchain was on PATH. rustc 1.99.1; Cargo 1.99.1. Installed agent versions recorded in [compatibility](compatibility.md); neither agent was invoked for model work.

- `cargo fmt --check`: pass.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: pass.
- `cargo test --workspace --locked --offline`: pass, 44 tests, 0 failures (4 accounting, 5 core, 16 store, 4 daemon writer, 10 socket integration, 2 offline CLI, 3 daemon CLI).
- `cargo build --workspace --locked --offline`: pass.
- Actual demo import: 501 events, 100 requests, 100 tools, 10,000 input, 2,000 output, 12,000 total tokens. Output reasoning subset: 500; visible output: 1,500. No incomplete calls in ordinary fixture.

Offline flags were used after dependencies were fetched. On this machine, prefix Cargo commands with `CARGO_HOME=/tmp/contextscope-cargo RUSTUP_HOME=/tmp/contextscope-rustup` and use `/tmp/contextscope-cargo/bin/cargo`; this temporary toolchain may be removed by host cleanup. A standard Rust installation can run the README commands unchanged.

## Remaining work and boundaries

[ADR 0001](decisions/0001-offline-scope.md) records the synchronous single-writer choice. The M0 event-contract task is complete: all specified event families are typed and JSON Schema validation runs during import. M0 as a whole is still partial: repository/worktree identity persistence, broader platform/feature CI, and full dependency/advisory/release audits remain. [Dependency metadata and the validator license/feature review](dependencies.md) are recorded. CI configuration is checked in but has not run on a remote runner.

M1 has started with O01; O02–O06 and M2–M8 remain unimplemented: live adapters and real wire fixtures, live telemetry, terminal/signal parity, TUI, artifact storage/leases/retrieval, projection policy/certificates, paired experiments, evidence graph/versioning, packaging, gateway, learned policies, and SDK. Later-family events are metadata observations; they do not implement those capabilities. Full specification tables for those subsystems remain deferred. Session resumes have explicit producer-epoch linkage; forks and calls shared across producers/epochs remain unsupported and are marked ambiguous or rejected as appropriate. Task outcome assertions retain observed/agent-reported/inferred provenance and optional oracle version; there is no oracle runner. Collector wall-clock lifecycle timestamps are available; monotonic durations are not. No real savings, quality, latency, or memory benchmarks exist. See [benchmarks](benchmarks.md), [compatibility](compatibility.md), and [threat model](threat-model.md).

## Session lifecycle follow-up

These initial rules are extended below for explicit session-resume chains.

Reports expose `session_status` (`incomplete`, `ended`, `unknown`) and nullable `started_at_unix_ms` / `ended_at_unix_ms` collector observations. EOF never implies an end, and an explicit end never asserts task success. Multiple lifecycle streams, repeated markers, missing pair sequence, or reversed sequence produce unknown state with unavailable timestamps. Collector wall-clock regression does not override producer sequence. Null means unavailable; no elapsed duration is inferred.

Database schema 2 adds lifecycle columns without rewriting source events and backfills schema-1 sessions from the ledger. Tests cover upgrades/reopens, late imports, deduplication, missing markers, ambiguous streams and clock regression. The ordinary fixture has no session-end event and is correctly incomplete despite all requests/tools being complete. [ADR 0002](decisions/0002-session-lifecycle.md) defines these semantics.

Validation: format, locked offline workspace build/test, and Clippy with warnings denied pass. The lifecycle demo reports 2 source events, explicit ended state, observed start 1700000000000 and end 1700000000001; zero model calls. M0 remains partial; this follow-up does not establish daemon or live adapter support.

## M0 event-contract follow-up

| Requirement | Implementation | Tests / fixture | Demonstration |
|---|---|---|---|
| All specified families, plus streaming and cancellation | [payloads/enums](../crates/cs-core/src/events.rs), [schema](../schemas/event-v1.json), [validation](../crates/cs-core/src/lib.rs) | [core conformance tests](../crates/cs-core/src/conformance.rs): every variant round trips; required fields, numeric limits, unknown/private fields, identity/capability invariants | import [event-contract fixture](../fixtures/event-contract.jsonl); inspect `contract --json` |
| Explicit resume graph, task outcomes, conservative call states | [reconciliation](../crates/cs-store/src/reconcile.rs), [store](../crates/cs-store/src/lib.rs) | [store conformance tests](../crates/cs-store/src/conformance.rs): late/reversed imports, missing parent, branches/cycles, contradictory terminals/outcomes, task links, provenance | inspect `contract` as Markdown; [sample](sample-event-contract.md) |
| Transactional migration 3 and metadata/task/epoch views | [migration 003](../crates/cs-store/migrations/003.sql), [migration driver](../crates/cs-store/src/lib.rs) | v1/v2 upgrades, reopen, immutable source preservation, failed-upgrade rollback, foreign keys, malformed import rollback | open an existing schema-1/2 ledger with `inspect` |
| Bounded JSONL parser and safe errors | [CLI](../crates/mertis/src/main.rs) | [subprocess tests](../crates/mertis/tests/cli.rs): private fields, invalid UTF-8, oversized records/files, reports, unchanged ledger after failure | README import/report commands |

Actual demo: 35 source events, all 35 event kinds, 3 requests, 3 tools, 1 failed and 1 cancelled of each call type, 1 explicitly observed task outcome with oracle version, 2 visibility gaps, 2 reported dropped events. Usage is correctly partial (only 1/3 requests has usage); no missing categories become zero. Original ordinary totals remain 100 requests/tools and 12,000 tokens. An existing schema-2 lifecycle-demo ledger opened successfully with unchanged totals.

Validation: locked offline workspace build/test and Clippy with warnings denied pass; format check passes. [ADR 0003](decisions/0003-complete-event-contract.md) defines the passive-observation and migration boundaries. All documentation examples use isolated storage and require no model calls.

## M1 O01 — authenticated local daemon

O01 is implemented for Linux x86_64. The daemon/writer dependency is now available without requiring a live adapter, model API key or installed service. The M0 offline exit demonstration remains satisfied, while repository/worktree identity persistence and broader platform/release conformance remain partial foundations. The next dependency-ordered M1 ticket is **O02, session launcher**; completing O01 does not mean M1's real-session exit criterion has been met.

| Requirement | Implementation | Evidence / demonstration |
|---|---|---|
| Owner authentication and capability/session binding | [runtime](../crates/cs-daemon/src/runtime.rs), [server](../crates/cs-daemon/src/server.rs), [protocol](../crates/cs-daemon/src/protocol.rs) | `handshake_rejects_missing_bad_credentials_and_unknown_versions`, `atomic_session_binding_schema_and_accounting_rejection`, `cap_batch_size_and_session_capabilities` |
| Bounded frame, queue, connections and deadlines | [protocol](../crates/cs-daemon/src/protocol.rs), [writer](../crates/cs-daemon/src/writer.rs), [socket tests](../crates/cs-daemon/tests/ipc.rs) | oversized/malformed-frame, drip-feed deadline, connection admission and locked-database queue saturation tests |
| Dedicated SQLite writer, atomic batches, restart | [writer](../crates/cs-daemon/src/writer.rs), [bounded store lock API](../crates/cs-store/src/lib.rs) | `append_batches_deduplicate_and_survive_restart`, `expired_queued_work_never_commits_and_counts_once`, `writer_lock_contention_obeys_the_configured_busy_timeout` |
| Safe bounded loss reporting | [health/spool](../crates/cs-daemon/src/writer.rs) | stalled-spool response/shutdown, queue/file limits, rotation and actual I/O-error tests; 64 KiB rolling metadata notice file, not event replay |
| CLI and runtime lifecycle | [CLI](../crates/mertis/src/main.rs), [subprocess tests](../crates/mertis/tests/daemon.rs) | start/status/stop, SIGTERM cleanup, explicit restart after forced process death; runtime modes/lock/symlink tests |
| Offline reproducible IPC demo | [Rust example](../crates/cs-daemon/examples/cached_session.rs), [usage/protocol docs](daemon.md) | README commands; cached fixture snapshot has 150 input tokens, source reimport deduplicates |

An independent read-only review identified blocking loss-spool I/O and double-counted expiry. Both had failing regression tests before fixes and now pass. A subsequent lock-contention failure was traced to deferred SQLite transaction upgrades bypassing the busy wait; immediate write reservations now exercise the configured timeout. [ADR 0004](decisions/0004-local-daemon-writer.md) records those choices. No code was committed, pushed, or added to the rename-only PR by this task.

Validation: full locked offline workspace tests, formatting, Clippy with warnings denied, and build. Real Unix-socket creation is prohibited by this session's sandbox (`Operation not permitted`), so the approved socket test suite runs outside that sandbox. Unsupported platforms explicitly refuse daemon commands; offline functionality remains separate. Cross-UID rejection is enforced in code but a negative multi-user integration test has not been run. Live adapters, subscriptions, transformations and artifact retrieval remain unavailable.

Actual O01 demo (2026-11-07): foreground daemon started in an isolated `/tmp` home; authenticated health returned protocol 1 / observe; Rust cached-session example persisted 7 events and reported 150 total input tokens. Repeating the example kept the event count at 7. Authenticated stop exited with status 0 and removed both socket and token. All 44 tests passed; format, locked offline workspace build/examples and all-target Clippy with warnings denied passed.
