# RustPLC

[![CI](https://github.com/logohere/rustplc/actions/workflows/ci.yml/badge.svg?branch=development)](https://github.com/logohere/rustplc/actions/workflows/ci.yml)
[![Dependency security](https://github.com/logohere/rustplc/actions/workflows/security.yml/badge.svg?branch=development)](https://github.com/logohere/rustplc/actions/workflows/security.yml)

A macOS-first Rust PLC editor, Structured Text compiler, and runtime compatibility layer based on pinned OpenPLC and STruC++ sources.

Includes project and Ladder/FBD editing, generated C++ compilation, local simulation, and direct OpenPLC Runtime v3/v4 deployment and debugging. Compatibility is defined by the [pinned upstream baseline](docs/UPSTREAM_BASELINE.md), not the latest OpenPLC release. Windows/Linux product packaging and real-time qualification are separate work.

This is not a safety-certified controller. Build success and simulator tests do not qualify it to operate physical equipment.

## Status and upstream relationship

RustPLC is a separate project, not an official OpenPLC distribution. OpenPLC and STruC++ are the reference upstreams for the [pinned compatibility baseline](docs/UPSTREAM_BASELINE.md); RustPLC development and contributions live here. There is no automatic synchronization or agreement that upstream projects will adopt RustPLC changes.

This is a source publication, not a packaged binary release. See the [verification scope](docs/RUSTPLC_PARITY_LEDGER.md), [security limitations](SECURITY.md), and [open work](https://github.com/logohere/rustplc/issues). This repository does not provide an MCP server.

## Build

Requires Git, Rust 1.96.0, Python 3, and a host C++ compiler. On macOS, install the Xcode Command Line Tools or Xcode for `clang++`. CI uses Rust 1.96.0; use that toolchain for the documented checks. macOS is the primary product host.

```sh
git clone --branch development https://github.com/logohere/rustplc.git
cd rustplc
rustup override set 1.96.0
```

Build the workspace, then launch the editor from source:

```sh
cargo build --locked --workspace
cargo run --locked --bin openplc-editor
```

Build the headless runtime or a local macOS app bundle:

```sh
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

The [verification scope](docs/RUSTPLC_PARITY_LEDGER.md) records preparation checks, gaps, and explicitly invoked heavy tests. The [differential harness](docs/DIFFERENTIAL_HARNESS.md) compares pinned upstream behavior. Neither substitutes for a current run.

## Project layout

| Path | Responsibility |
| --- | --- |
| `crates/compat-core` | Runtime compatibility, generated-library loading, archive validation, and headless commands |
| `crates/compiler-core` | Structured Text and graphical compilation, C++ generation, and runtime support |
| `crates/openplc-editor-macos` | Desktop editor, project/library services, simulation, and remote-runtime integration |
| `fixtures` | Reproducible runtime and compatibility inputs |
| `scripts` | Build, source-policy, and differential verification tools |
| `docs` | Source pins, verification scope, maintainer rules, and release procedures |

## Contribute

Read [CONTRIBUTING.md](CONTRIBUTING.md). Start with an existing issue or a reproducible new report, then use a scoped branch, tests, and a reviewed PR into `development`. Releases promote reviewed changes to `main`.

Report RustPLC bugs and propose RustPLC changes in this repository. A fix intended for OpenPLC or STruC++ needs separate reproduction against that upstream, its own contribution-rule review, and the owner's manual review before an agent submits it.

Report vulnerabilities [privately](https://github.com/logohere/rustplc/security/advisories/new). See [SECURITY.md](SECURITY.md) and [maintainer rules](docs/MAINTAINERS.md).

## License

Mixed-license: applicable pinned runtime-derived compatibility code is MIT; the compiler and editor crates are GPL-3.0-only. Vendored code retains its own terms. The compatibility-code MIT license does not relicense GPL-derived components.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), [LICENSE](LICENSE), and [LICENSES](LICENSES).

Created and maintained by Justin DiClemente.
