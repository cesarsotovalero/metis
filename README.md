# Mertis

An offline Rust observer for **synthetic** coding-agent session fixtures. Implements the offline foundation and Linux daemon IPC task O01 from [the specification](docs/specification.md), not an R0/R1 release. No agent behavior is modified. No network requests or model credentials are used at runtime.

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

The ordinary trace has 100 synthetic requests and tools, 10,000 input and 2,000 output tokens. Reasoning is a subset of output: 500 reasoning plus 1,500 visible output, never 2,500 output. `cached.jsonl` revises one request from 100 to 150 input; its final categories are 90 uncached and 60 cache-read. `lifecycle.jsonl` has explicit start/end markers. Reports distinguish ended, incomplete, and ambiguous session state; null lifecycle timestamps mean unavailable. The ordinary trace has no end marker, so its session remains incomplete despite complete calls. `interrupted.jsonl` has one unfinished request and tool and a duplicate source record. Missing usage is unavailable, not zero. Costs are unavailable without a price snapshot; no efficiency savings are claimed.

The [event-contract fixture](fixtures/event-contract.jsonl) covers all 35 typed event variants, including resume links, task outcomes with evidence provenance, streaming/cancellation, compaction, recovery observations, and policy observations. Recording these events does not perform transformations, retrieval, or policy activation. JSON and Markdown reports include event-kind counts and task/call state. See the [sample report](docs/sample-event-contract.md) and [wire-contract notes](docs/event-contract.md).

```sh
cargo run --locked -p mertis -- --home /tmp/mertis-contract import --agent fixture --path fixtures/event-contract.jsonl
cargo run --locked -p mertis -- --home /tmp/mertis-contract inspect contract --json
```

`MERTIS_HOME` or `--home` chooses storage; otherwise `$XDG_DATA_HOME/mertis` or `$HOME/.local/share/mertis` is used. Linux is the only tested platform. Import accepts only schema-v1 synthetic metadata, at most 32 MiB per file and 64 KiB per line. Unknown fields, event kinds, versions, and conflicting duplicate identities fail the import. Events contain no prompt, source, arguments, or output bodies. Treat fixture identifiers as metadata, not arbitrary text fields.

Reports use immutable source events and explicit cumulative/delta semantics. Producer sequence controls within-producer order; different producers have no authoritative total order. Shared request identities across producers are not supported in this slice; see [compatibility](docs/compatibility.md).

For troubleshooting, invalid input exits nonzero without a partial file import. Newer database versions are refused. Preserve the data directory before upgrading; no destructive migrations are implemented. To uninstall, remove the installed binary and the selected data directory after backing it up. No global agent settings are installed.

## Local daemon (Linux)

Start the authenticated observer daemon in the foreground:

```sh
cargo run --locked -p mertis -- --home /tmp/mertis-daemon-demo daemon start
```

In another terminal, query health, send a checked-in synthetic fixture through IPC, and stop it:

```sh
cargo run --locked -p mertis -- --home /tmp/mertis-daemon-demo daemon status
cargo run --locked -p cs-daemon --example cached_session -- /tmp/mertis-daemon-demo
cargo run --locked -p mertis -- --home /tmp/mertis-daemon-demo daemon stop
```

See [daemon protocol, limits and timeout behavior](docs/daemon.md). This supplies M1 O01's local authenticated transport and writer; live adapters, the session launcher and TUI remain unimplemented. Runtime files are private to the owner, and no service or global agent configuration is installed.
