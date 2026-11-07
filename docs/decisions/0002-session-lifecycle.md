# ADR 0002: conservative session lifecycle reconciliation

Accepted 2026-11-05. Continue M0 F02/F03/F04 with materialized session lifecycle before starting M1 infrastructure.

The existing schema-v1 start/end markers provide no process resume or task outcome information. A single end marker means ended, not successful. No end marker means incomplete, including a trace containing only completed calls. A missing start does not invalidate an observed end; its timestamp remains unavailable.

Reconcile one producer/epoch lifecycle stream using producer sequence, independent of import order and collector clock regression. Multiple streams, repeated starts/ends, unordered start/end pairs, or an end preceding a start in sequence mean unknown. Do not guess process lineage from wall-clock timestamps. Ambiguous lifecycle timestamps are null. Collector observation times are explicitly labeled and never used as monotonic durations.

Additive database schema 2 stores status and nullable start/end observations, preserving schema-1 immutable events and migration text. Backfill existing sessions during upgrade; subsequent imports update the materialized lifecycle in the same transaction as events and call summaries. Reports continue to fold immutable events as the authority. No wire version change is needed because event shapes are unchanged. The JSON report gains additive fields.

Resumes, forks, cancellation, task outcomes, authoritative source timestamps, and monotonic duration measurements need explicit event contracts in later slices. This conservative subset does not claim the complete specification lifecycle model.
