# Synthetic event contract v1

The [JSON Schema](../schemas/event-v1.json) is the wire boundary. [Rust types](../crates/cs-core/src/events.rs) define every supported payload, and `EventEnvelope::validate` executes the embedded schema and cross-field identity checks. `cargo test -p cs-core --locked` independently validates the checked-in fixtures, round trips every kind, compares schema/fixture variant coverage, removes every required payload field, and checks malformed/privacy inputs.

The envelope carries event/session/producer/epoch identity, optional producer sequence, observed and optional source wall-clock timestamps, optional task/turn/correlation IDs, synthetic visibility, and a typed payload. Sequence is limited to SQLite's signed integer range; token/byte counters support u64. Optional timestamps and usage categories are null/absent when unavailable. Opaque strings are 1–128 Unicode characters without control characters. Hash fields require 64 lowercase hexadecimal characters. Files are capped at 32 MiB and JSON records at 64 KiB, with LF/CRLF delimiters outside the record cap. Runtime schema references cannot access the network or filesystem.

| Family | Supported kinds |
|---|---|
| Lifecycle | session_started, session_resumed, session_ended, task_started, task_outcome_recorded |
| Model | request_started, request_streaming, request_completed, request_failed, request_cancelled, usage_observed, model_changed |
| Tools | tool_started, tool_completed, tool_failed, tool_cancelled |
| Evidence | artifact_stored, file_version_observed, artifact_superseded, requirement_recorded |
| Context | compaction_observed, context_snapshot_observed, representation_proposed, representation_committed, representation_rejected |
| Recovery | retrieval_requested, retrieval_completed, retrieval_unavailable |
| Integrity | visibility_gap, adapter_capability_changed, queue_overflow, reconciliation_result |
| Policy | policy_activated, policy_disabled, experiment_assignment |

Use the schema for exact required fields and enum values. All 35 shapes appear in [event-contract.jsonl](../fixtures/event-contract.jsonl). Metadata describes assertions and references; it never contains prompt, source, arguments, output bodies, arbitrary extensions or private reasoning. Unknown fields, kinds and versions fail the file import atomically. Empty lifecycle objects obey the same rule as payloads with fields.

`session_resumed.previous_epoch` explicitly links a new epoch to the preceding epoch within the same producer. Self-links fail validation; missing parent, branches, cycles, repeated opening/end markers and unresolved ordering produce unknown session state. Late imports can resolve missing parents. Epoch names and timestamps are never used to invent a total order. The current epoch determines ended versus incomplete. An old epoch end does not end a newly resumed process. Import EOF never supplies a missing end.

Request/tool streams support incomplete, streaming (requests), completed, failed, cancelled and unknown states. Cancellation is separate from failure. Distinct contradictory terminal events become unknown, counted in the incomplete total and also the ambiguous total. Repeated identical source records deduplicate; different calls with identical contents do not. Calls sharing an identity across producers/epochs fail atomically until a real reconciliation adapter exists.

Task outcomes carry `passed`, `failed`, `cancelled` or `unknown`, evidence kind (`observed`, `agent_reported`, `inferred`) and an optional oracle version. These are assertions supplied by the synthetic producer. The observer does not run an oracle or promote an inferred/agent assertion into an independently verified outcome. Missing or conflicting task outcomes produce an unavailable measurement. Task identities in the envelope and lifecycle payload must agree when both are supplied. Calls can refer to tasks before the task-start event; persistence creates a placeholder with unavailable outcome. Conflicting call-to-task assignments fail atomically.

Policy activation, full-local artifact metadata, verified capability assertions, representation commits and retrieval completions are passive observations. They cannot change this importer's metadata capture, observe mode or permissions, and they do not create artifact bytes, grants, projections or recovery services. A capability marked verified requires an evidence reference even in a synthetic observation. R1 delivery visibility, artifact recovery and real adapter conformance remain future work.

Schema 3 adds typed event metadata, task outcome/provenance records, task-linked requests/tools and producer epochs. The source ledger remains immutable and authoritative. Schema 1/2 upgrades, migration integrity records and materialized-view backfills share one transaction. Referenced tasks are session-scoped using composite foreign keys. Future subsystem observations stay in the ledger and kind counts until their dedicated implementation exists.

Run an isolated demonstration:

```sh
cargo run --locked -p mertis -- --home /tmp/mertis-contract import --agent fixture --path fixtures/event-contract.jsonl
cargo run --locked -p mertis -- --home /tmp/mertis-contract inspect contract --json
cargo run --locked -p mertis -- --home /tmp/mertis-contract inspect contract
```

The [sample report](sample-event-contract.md) contains 35 source events, three requests/tools, one failure and cancellation of each type, one task outcome, and explicit missing usage coverage. It makes no optimization or cost claim.
