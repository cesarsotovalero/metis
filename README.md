# Mertis

An offline Rust observer for **synthetic** coding-agent session fixtures. This is the first implementation assignment from [the specification](docs/specification.md), not an R0/R1 release. No agent behavior is modified. No network requests or model credentials are used at runtime.

Requires Rust 1.99.1 (pinned), a C compiler for bundled SQLite, and Cargo registry access for the first build. Run from this directory:

```sh
cargo build --workspace --locked
cargo run --locked -p mertis -- --home /tmp/mertis-demo import --agent fixture --path fixtures/ordinary.jsonl
cargo run --locked -p mertis -- --home /tmp/mertis-demo inspect ordinary --json
cargo run --locked -p mertis -- --home /tmp/mertis-demo inspect ordinary
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The ordinary trace has 100 synthetic requests and tools, 10,000 input and 2,000 output tokens. Reasoning is a subset of output: 500 reasoning plus 1,500 visible output, never 2,500 output. `cached.jsonl` revises one request from 100 to 150 input; its final categories are 90 uncached and 60 cache-read. `interrupted.jsonl` has one unfinished request and tool and a duplicate source record. Missing usage is unavailable, not zero. Costs are unavailable without a price snapshot; no efficiency savings are claimed.

`MERTIS_HOME` or `--home` chooses storage; otherwise `$XDG_DATA_HOME/mertis` or `$HOME/.local/share/mertis` is used. Linux is the only tested platform. Import accepts only schema-v1 synthetic metadata, at most 32 MiB per file and 64 KiB per line. Unknown fields, event kinds, versions, and conflicting duplicate identities fail the import. Events contain no prompt, source, arguments, or output bodies. Treat fixture identifiers as metadata, not arbitrary text fields.

Reports use immutable source events and explicit cumulative/delta semantics. Producer sequence controls within-producer order; different producers have no authoritative total order. Shared request identities across producers are not supported in this slice; see [compatibility](docs/compatibility.md).

For troubleshooting, invalid input exits nonzero without a partial file import. Newer database versions are refused. Preserve the data directory before upgrading; no destructive migrations are implemented. To uninstall, remove the installed binary and the selected data directory after backing it up. No global agent settings are installed.
