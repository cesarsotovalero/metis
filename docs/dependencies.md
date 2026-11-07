# Locked dependency inventory

Recorded 2026-11-07 from `cargo metadata --locked --offline --format-version 1` after resolving the JSON Schema validator. This includes platform-specific and optional packages in the resolved metadata, not only the Linux binary. First-party crates use MIT.

The added direct dependency `jsonschema` 0.60.2 declares MIT. Its default features are disabled, including HTTP/file reference resolution; the embedded draft-2020-12 schema uses local references only. See the [upstream API and feature documentation](https://docs.rs/jsonschema/0.60.2/jsonschema/). Dependencies remain pinned by Cargo.lock. The inventory contains only permissive license choices (choose MIT where offered; Unicode-3.0 applies in addition where declared). Preserve upstream copyright/license notices when packaging. Bundled SQLite licensing and any non-Cargo native dependencies also need release-packaging review.

This is an SPDX metadata inventory and direct-dependency license review. A vulnerability/advisory scan, source-by-source legal audit, binary notice bundle, and platform packaging audit have not been run. No runtime networking is introduced by schema validation.

M1 O01 adds Linux-only safe `nix` 0.31.3 (MIT) peer credential/user APIs, `getrandom` 0.3.4 (MIT OR Apache-2.0) with std error support, and `signal-hook` 0.3.18 (Apache-2.0 OR MIT) for graceful foreground shutdown. No first-party unsafe code is introduced. Standard-library file locks, threads and bounded channels supply scheduling/runtime ownership; no async runtime or TCP stack is added to the application.

| Package | Version | Declared license |
|---|---|---|
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| bit-set | 0.8.0 | Apache-2.0 OR MIT |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| borrow-or-share | 0.2.4 | MIT-0 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytecount | 0.6.9 | Apache-2.0/MIT |
| cc | 1.11.0 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| cfg_aliases | 0.2.2 | MIT |
| clap | 4.6.7 | MIT OR Apache-2.0 |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| data-encoding | 2.11.1 | MIT |
| email_address | 0.2.9 | MIT |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 |
| fancy-regex | 0.19.2 | MIT |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| fluent-uri | 0.4.1 | MIT |
| foldhash | 0.2.0 | Zlib |
| fraction | 0.17.0 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| hashlink | 0.11.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| js-sys | 0.3.107 | MIT OR Apache-2.0 |
| jsonschema | 0.60.2 | MIT |
| jsonschema-regex | 0.60.2 | MIT |
| jsonschema-value | 0.60.2 | MIT |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| libsqlite3-sys | 0.36.0 | MIT |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| memoffset | 0.9.1 | MIT |
| micromap | 0.3.0 | MIT |
| nix | 0.31.3 | MIT |
| num | 0.4.3 | MIT OR Apache-2.0 |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 |
| num-cmp | 0.1.0 | MIT/Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-iter | 0.1.46 | MIT OR Apache-2.0 |
| num-rational | 0.4.2 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| outref | 0.5.2 | MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| redox_syscall | 0.5.18 | MIT |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 |
| referencing | 0.60.2 | MIT |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| rsqlite-vfs | 0.1.1 | MIT |
| rusqlite | 0.38.0 | MIT |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| signal-hook | 0.3.18 | Apache-2.0/MIT |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 |
| smallvec | 1.16.4 | MIT OR Apache-2.0 |
| sqlite-wasm-rs | 0.5.5 | MIT |
| strsim | 0.11.1 | MIT |
| strum | 0.28.0 | MIT |
| strum_macros | 0.28.0 | MIT |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| unicode-general-category | 1.1.0 | Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| uuid-simd | 0.8.0 | MIT |
| vcpkg | 0.2.15 | MIT/Apache-2.0 |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| vsimd | 0.8.0 | MIT |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.130 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zmij | 1.0.23 | MIT |
