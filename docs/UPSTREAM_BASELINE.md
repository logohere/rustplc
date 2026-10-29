# Upstream Baseline

## Pinned source

- Repository: https://github.com/Autonomy-Logic/openplc-runtime
- Version file: `v4.1.10`
- Commit: `bf82b1b661fd95c9899969f629d692b77b4e1454`
- Commit date: `2026-08-11T17:22:49-03:00`
- License: MIT

All parity evidence starts from this commit. A baseline update requires a separate issue and a full differential run.

These pins define the source comparison baseline, not a release or complete-parity claim. Later OpenPLC releases require separate comparison and review before updating the baseline.

## Generated-program toolchain

- OpenPLC Editor commit: [`32e324c7c8278cbf95e8b519fb6b935312ffcfc4`](https://github.com/Autonomy-Logic/openplc-editor/commit/32e324c7c8278cbf95e8b519fb6b935312ffcfc4)
- OpenPLC Editor license at the pinned commit: GPL-3.0
- STruC++ tag: [`v0.6.2`](https://github.com/Autonomy-Logic/STruCpp/releases/tag/v0.6.2)
- STruC++ commit: [`e48bc707dcce00572c07b07160a4437a5e5aed45`](https://github.com/Autonomy-Logic/STruCpp/commit/e48bc707dcce00572c07b07160a4437a5e5aed45)
- STruC++ license at the pinned commit: GPL-3.0

The pinned runtime header says it mirrors STruC++ v0.4.5, but that tag's `ProgramBase` ends after vtable slot 3. The runtime header defines `sync_in`, `sync_out`, and slot-6 `located_range`, so literal v0.4.5 cannot satisfy the ABI the runtime calls. The Editor commit above pins v0.6.2 specifically for located-global synchronization; v0.6.2 contains those slots and is therefore the effective generated-program ABI baseline.

This pin prevents an impossible v0.4.5 fixture. It does not prove binary parity; a generated v0.6.2 library still must pass the differential ABI fixture.

## License split

This repository is mixed-license because the authoritative sources are mixed-license:

- the OpenPLC Runtime-derived Rust compatibility/runtime work follows the pinned MIT runtime source;
- the RustPLC Editor desktop/editor and owned compiler contain direct GPL-3.0-derived/adapted work from the pinned OpenPLC Editor/STruC++ sources and are marked GPL-3.0 in their crate manifests;
- the exact GPL-3.0 text is stored at `LICENSES/GPL-3.0.txt`.

Do not describe the entire repository as MIT. Public release packaging requires a final provenance/license review.

## Reproduce the inventory

The machine-readable inventory is [`baselines/openplc-runtime-v4.1.10.json`](../baselines/openplc-runtime-v4.1.10.json). Validate its schema without network access, or validate a checkout at the pinned commit:

```bash
python3 scripts/verify_upstream_baseline.py
python3 scripts/verify_upstream_baseline.py --source /path/to/openplc-runtime
python3 scripts/verify_upstream_baseline.py \
  --source /path/to/openplc-runtime \
  --editor-source /path/to/openplc-editor \
  --strucpp-source /path/to/strucpp
```

The verifier excludes the pinned SOEM gitlink from the regular-file list and checks its object ID separately.

## Port surface

| Upstream area | Rust target | Required evidence |
| --- | --- | --- |
| `core/src/plc_app` | Lifecycle, scan scheduler, process image, debug journal, watchdog, IPC | State and scan traces match |
| `core/src/drivers` | Plugin configuration and I/O lifecycle | Driver contract tests match |
| `core/src/lib/strucpp_abi.hpp` | Uploaded-program ABI boundary | Fixture library loads and executes identically |
| `webserver` | REST, authentication, upload, build orchestration, debug bridge | Contract and security tests match |
| `scripts` | Compilation and artifact lifecycle | Editor package produces equivalent loadable output |
| `windows` and containers | Packaging and startup behavior | Platform CI and smoke tests pass |

## Architecture constraint

OpenPLC Runtime v4 is a dual-process system: a Python/Flask API server and a C/C++ real-time runtime communicate over Unix sockets. The first Rust releases preserve those observable boundaries. Internal consolidation is deferred until compatibility is proven.
