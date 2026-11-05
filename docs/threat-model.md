# Initial threat model

Trust user-selected storage configuration; treat input fixture JSON as untrusted. Strict typed fields reject bodies/extensions, enforce schema version, bound files/lines, and use SQL parameters. Never execute fixture contents. Errors avoid echoing fixture bodies. Identities are metadata: operators must not put secrets in them.

SQLite and sidecars live under an owner-only directory on Unix. This is not encryption. A compromised same-user process, malicious symlink in a user-chosen data path, or direct database edits are outside this initial local slice. Do not use a shared or attacker-controlled home directory. Runtime accepts no network clients and offers no retrieval. Future authentication, path hardening, retention, and destructive migration controls are required before live deployment.
