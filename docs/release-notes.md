# 0.1.0 offline foundation

Initial synthetic fixture importer and observer. Supports deterministic usage normalization, immutable event persistence, deduplication, restart reconciliation, JSON reports, and Markdown reports. Provides five checked-in traces and an offline conformance test suite.

This is a development snapshot, not an R0/R1 release. Only Linux x86_64 has been exercised. No agent hooks, gateway, retrieval, transformations, live billing, or savings claims. Data capture is typed synthetic metadata only. No automatic migrations from future schemas; preserve a backup before upgrades.

Session lifecycle follow-up: JSON and Markdown now report explicit session status and collector start/end observations. Additive database migration 002 backfills existing ledgers. Missing/ambiguous lifecycle evidence stays unavailable; no task success or elapsed-time claim is inferred.

M0 event contract: all 35 metadata variants are typed and validated by the embedded JSON Schema, including task outcomes, explicit resume links, streaming/cancellation, evidence/context/recovery/integrity/policy observations. Reports retain outcome provenance, show per-kind counts and reported overflow losses, and distinguish contradictory terminal states. Schema 3 adds metadata/task/epoch views and task-linked calls. Upgrades and backfills now share one transaction; immutable source events are preserved. Empty lifecycle variants reject extra fields. JSONL allocation is bounded before reading oversized records. Later-family events remain passive observations.

M1 O01: Linux foreground daemon with authenticated private Unix IPC, negotiated capabilities, session-bound synthetic ingestion/snapshots, health and graceful shutdown. Adds a dedicated bounded SQLite writer, explicit busy deadlines, bounded loss-notice reporting, singleton/stale endpoint protection, and SIGINT/SIGTERM handling. `mertis daemon start|status|stop` and the Rust cached-session client demonstrate it offline. Other daemon platforms, live integrations, launcher, TUI, subscriptions, transformations and recovery remain unavailable.
