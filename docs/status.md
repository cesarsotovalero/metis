# Implementation status — 2026-11-05

Implemented the **section 25 first assignment**, with scoped deviations recorded below. This is an offline foundation, not completion of M0–M8 or an R0 release. No live integration, optimization, model calls, global agent configuration, or deployment occurred.

| Requirement | Implementation | Tests / fixture | Demonstration |
|---|---|---|---|
| Minimal four-crate workspace, pinned toolchain | [workspace](../Cargo.toml), [toolchain](../rust-toolchain.toml), [lock](../Cargo.lock), [CI](../.github/workflows/ci.yml) | full workspace checks | `cargo build --workspace --locked` |
| Typed v1 event envelope, explicit measurement provenance | [core](../crates/cs-core/src/lib.rs), [wire schema](../schemas/event-v1.json) | `wire_round_trip_and_strict_privacy_boundary`; all JSONL fixtures | import any fixture below |
| Cache/reasoning accounting, cumulative revisions, deltas, missingness, overflow | [accounting](../crates/cs-accounting/src/lib.rs) | four accounting tests; [cached](../fixtures/cached.jsonl) | `contextscope import --agent fixture --path fixtures/cached.jsonl` then `contextscope inspect cached --json` |
| SQLite WAL, foreign keys, numbered migration, immutable events, materialized calls | [store](../crates/cs-store/src/lib.rs), [migration](../crates/cs-store/migrations/001.sql) | `migration_integrity_immutability_and_materialized_records`, `restart_and_newer_schema` | reimport the same trace after restarting CLI |
| Atomic import, source deduplication, late events, conflicts | [store](../crates/cs-store/src/lib.rs) | `late_events_reconcile_and_source_duplicates_dedup`, `malformed_rollback_and_conflict`; [interrupted](../fixtures/interrupted.jsonl) | import interrupted twice; inspect interrupted |
| 100-turn offline totals and synthetic labeling | [ordinary fixture](../fixtures/ordinary.jsonl), [CLI](../crates/contextscope/src/main.rs) | `dedup_and_100_turn_totals`, [CLI subprocess test](../crates/contextscope/tests/cli.rs) | README commands |
| JSON and Markdown report, provenance and unavailable cost | [report](../crates/cs-core/src/lib.rs), [sample](sample-session.md) | CLI subprocess test and store accounting assertions | `contextscope inspect ordinary --json` or `contextscope inspect ordinary` |

Use `target/debug/contextscope` after building, or the `cargo run --locked -p contextscope --` prefix from [README](../README.md). All demonstration imports should specify `--home /tmp/contextscope-demo` before the subcommand, or set `CONTEXTSCOPE_HOME`, to isolate test data.

## Validation

Environment: Linux x86_64. Installed isolated Rust under `/tmp/contextscope-cargo` and `/tmp/contextscope-rustup`; no existing Rust toolchain was on PATH. rustc 1.99.1; Cargo 1.99.1. Installed agent versions recorded in [compatibility](compatibility.md); neither agent was invoked for model work.

- `cargo fmt --check`: pass.
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: pass.
- `cargo test --workspace --locked --offline`: pass, 12 tests, 0 failures (1 CLI subprocess, 4 accounting, 1 core, 6 store).
- `cargo build --workspace --locked --offline`: pass.
- Actual demo import: 501 events, 100 requests, 100 tools, 10,000 input, 2,000 output, 12,000 total tokens. Output reasoning subset: 500; visible output: 1,500. No incomplete calls in ordinary fixture.

Offline flags were used after dependencies were fetched. On this machine, prefix Cargo commands with `CARGO_HOME=/tmp/contextscope-cargo RUSTUP_HOME=/tmp/contextscope-rustup` and use `/tmp/contextscope-cargo/bin/cargo`; this temporary toolchain may be removed by host cleanup. A standard Rust installation can run the README commands unchanged.

## Remaining work and boundaries

[ADR 0001](decisions/0001-offline-scope.md) records the synchronous single-writer choice and limited schema. M0 is partial: all event families, repository/task domain entities, a daemon writer actor, JSON Schema validator execution, dependency/license audit, and broader CI conformance remain. The checked-in wire schema is a reference; Serde and semantic validation enforce input at runtime. No independent JSON Schema validation engine was run. CI configuration is checked in but has not run on a remote runner.

M1–M8 remain unimplemented: real adapters and sanitized real wire fixtures, live telemetry, terminal/signal parity, TUI, daemon authentication, artifacts/leases/retrieval, projection policy/certificates, paired experiments, evidence graph/versioning, packaging, gateway, learned policies, and SDK. Full specification minimum schema is intentionally deferred beyond the four initial tables. Session state is not yet fully materialized. Cross-producer/epoch shared calls are rejected rather than double-counted. Timing and task outcomes are not reported. No real savings, quality, latency, or memory benchmarks exist. See [benchmarks](benchmarks.md), [compatibility](compatibility.md), and [threat model](threat-model.md).
