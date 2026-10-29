# RustPLC

A macOS-first Rust PLC editor, Structured Text compiler, and runtime compatibility layer based on pinned OpenPLC and STruC++ sources.

Includes project and Ladder/FBD editing, generated C++ compilation, local simulation, and direct OpenPLC Runtime v3/v4 deployment and debugging. Compatibility is defined by the [pinned upstream baseline](docs/UPSTREAM_BASELINE.md), not the latest OpenPLC release. Windows/Linux product packaging and real-time qualification are separate work.

This is not a safety-certified controller. Build success and simulator tests do not qualify it to operate physical equipment.

## Build

Install Rust and a host C++ compiler. macOS is the primary product host.

```sh
cargo build --locked --workspace
cargo build --locked --release --bin rustplc
./scripts/build_macos_app.sh
```

The packaging script downloads and hash-verifies the pinned Arduino CLI and ad-hoc signs the macOS bundle. It does not publish or notarize a release.

Run a generated compatible PLC library without automatically starting it:

```sh
./target/release/rustplc --program /absolute/path/libplc.dylib --safe-mode
```

The default socket is `$TMPDIR/rustplc-runtime.sock`; `--socket` overrides it.

## Check

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
python3 scripts/check_editor_css.py
python3 scripts/check_unsafe_boundaries.py
python3 scripts/check_production_panics.py
python3 scripts/check_durable_writes.py
python3 scripts/check_dependency_floors.py
python3 scripts/check_versioning.py
python3 scripts/check_public_source.py
```

The [parity ledger](docs/RUSTPLC_PARITY_LEDGER.md) records historical evidence and explicitly invoked heavy tests. The [differential harness](docs/DIFFERENTIAL_HARNESS.md) compares pinned upstream behavior. Neither substitutes for a current run.

Private lab sidecars, plant models, tag maps, and site-specific contracts are excluded. This standalone source export does not provide an MCP server.

## Contribute

Read [CONTRIBUTING.md](CONTRIBUTING.md). Start with an existing issue or a reproducible new report, then use a scoped branch, tests, and a reviewed PR into `development`. Releases promote reviewed changes to `main`.

Report vulnerabilities [privately](https://github.com/logohere/rustplc/security/advisories/new). See [SECURITY.md](SECURITY.md) and [maintainer rules](docs/MAINTAINERS.md).

## License

Mixed-license: applicable pinned runtime-derived compatibility code is MIT; the compiler and editor crates are GPL-3.0-only. Vendored code retains its own terms. The compatibility-code MIT license does not relicense GPL-derived components.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), [LICENSE](LICENSE), and [LICENSES](LICENSES).

Created and maintained by Justin DiClemente.
