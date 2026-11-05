# ADR 0001: implement the first assignment before live integration

Accepted 2026-11-05. The specification section 25 defines the initial executable. Implement only core, accounting, store, and CLI. Do not create unused milestone crates. Synthetic fixture usage is reported evidence of the fixture, never live usage. Reject unsupported schema fields instead of interpreting unknown data as authorization.

The offline importer owns one synchronous SQLite connection and one file transaction; no Tokio runtime or concurrent writer actor is needed until daemon M1. Reports reconstruct lifecycle and accounting from immutable events sorted by producer/epoch/sequence, with collector order as a fallback. Requests and tool-call materialized tables rebuild in the same transaction as import; the source event fold remains authoritative. Cross-producer request reconciliation is unavailable and must not be used to claim live coverage.

Migrations store exact SQL text for integrity comparison instead of a compact checksum; an exact comparison avoids hash collisions. Initial migration is transactional; destructive migration backup and full schema tables are deferred. No application-specific repository identity exists yet.

Privacy defaults to typed synthetic metadata. No arbitrary extension objects or raw bodies are retained. Artifact capture/recovery and transformations remain unavailable.
