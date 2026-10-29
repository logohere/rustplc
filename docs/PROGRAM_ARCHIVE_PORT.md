# Program archive port slice

## Source contract

- OpenPLC Runtime commit: `bf82b1b661fd95c9899969f629d692b77b4e1454`
- Runtime source: `webserver/plcapp_management.py` (`analyze_zip` and `safe_extract`)
- OpenPLC Editor commit: `32e324c`
- Rust target: `crates/compat-core/src/program_archive.rs`
- Differential fixture: `fixtures/program-archive.json`
- Differential adapter: `scripts/program_archive_trace.py`

The Rust slice validates an uploaded ZIP completely before creating its staging directory, then streams regular files into a new staging tree. It never replaces an existing destination. Extraction, CRC, size, or I/O failure removes the newly created staging tree.

## Preserved behavior

The differential trace preserves the pinned accepted layouts: flat projects, a single forward-slash root that is stripped, multiple roots, macOS metadata filtering, Windows separator normalization, and mixed separators. Root detection intentionally occurs on the raw ZIP filename before backslash normalization because the pinned `safe_extract` performs those operations in that order.

The pinned limits remain 10 MiB per uncompressed file, 50 MiB total uncompressed data, a 1000:1 maximum compression ratio, and the blocked extensions `.exe`, `.dll`, `.sh`, `.bat`, `.js`, `.vbs`, and `.scr`.

## Explicit hardening differences

Rust adds trust-boundary checks that the pinned Python implementation does not provide:

- the ZIP file itself is limited to 10 MiB and must be a regular file;
- archive entry count is limited to 4096;
- path traversal is rejected by normalized path components, while safe names such as `foo..bar.cpp` remain valid;
- encrypted entries, unsupported compression methods, duplicate normalized output paths, symlinks, and other non-regular archive entries are rejected before staging;
- non-empty entries reporting zero compressed bytes are rejected as an invalid compression ratio;
- only stored, deflate, bzip2, and LZMA readers are enabled;
- the staging parent must be a real directory rather than a symlink;
- output files use create-new semantics and an extracted-size bound;
- extraction failure rolls back the new staging tree without changing a prior generated tree.

These are deliberate hardening differences, not claims that the pinned Python implementation rejects the same malformed inputs. In particular, pinned `analyze_zip` rejects any filename containing the substring `..`; the Rust boundary rejects an actual parent component instead.

## Dependency

`zip` is pinned to `8.0.0` with default features disabled and only `bzip2`, `deflate-flate2-zlib-rs`, and `lzma` enabled. The crate is MIT licensed and is recorded in `THIRD_PARTY_NOTICES.md`.

## Evidence

Local macOS evidence on 2026-09-02:

- `cargo fmt --all -- --check` passes;
- `cargo clippy --workspace --all-targets -- -D warnings` passes;
- all 13 `program_archive` integration tests pass;
- the pinned OpenPLC/Rust `program-archive` differential trace passes;
- a pinned STruC++ generated archive stages as 5 files / 11,662 bytes;
- two builds from the staged tree produce SHA-256 `8eb66d47e31df06e2db473b4cad2c60ff814d7a12e181c2951d556c144d63ccf`;
- both ignored generated-program ABI/load tests pass against that staged build.

The GitHub workflow repeats archive differential evidence on Linux and runs the archive tests through the workspace matrix on macOS, Linux, and Windows. The generated-program job repeats ZIP staging and ABI build/load evidence on macOS and Linux. Platform evidence is only considered complete when the PR checks are green.

The current `main` baseline has an unrelated intermittent macOS Unix-socket test failure; this slice does not alter that server code.

## Deliberate boundary

This slice does not implement multipart parsing, REST/JWT state, compiler invocation policy, final generated-tree promotion, Python blocks, VPP/plugin configuration, runtime start/stop, network listeners, driver I/O, or actuator writes.
