# RustPLC verification scope

This standalone source export is not a certified controller or a verified binary release. Source pins identify the comparison targets; they do not prove complete behavioral parity.

## Source checks run during preparation

On macOS with Rust 1.96.0:

- Workspace/all-target tests, formatting, and Clippy with warnings denied.
- Real generated-PLC build, load, start, debug read, force, release, and stop.
- Model workflow replay against real project/runtime services.
- Real OSCAT CODESYS library import and native compilation.
- CSS selector coverage, unsafe-boundary, production-panic, durable-write, dependency-floor, version, and public-source checks.
- Source and Git-history secret scanning, plus vendored-source hash verification.

These are scoped test results, not proof that every compiler feature, editor interaction, remote controller, or failure mode works. Hosted CI is separate evidence and must pass for the exact PR revision before merge.

## Not established for this export

- Complete differential parity against all pinned OpenPLC/STruC++ behavior.
- Rendered editor interaction, accessibility, or packaged-app acceptance.
- Arduino toolchain/hardware acceptance or physical controller qualification.
- Windows/Linux product packaging, notarization, or release signing.
- Safety certification, production readiness, or absence of all vulnerabilities.

No binary release or version tag is included in this source preparation. Run the applicable native, differential, rendered-UI, packaging, hardware, and security checks before making a corresponding release claim. Record commands, exact revisions, failures, and skipped checks in the release review.

## Reproduce

Use the README checks and explicit heavy tests:

```sh
cargo test --locked -p open-rust-plc-editor \
  local_runtime::tests::builds_starts_and_stops_through_the_app_runtime_service \
  -- --ignored
cargo test --locked -p open-rust-plc-editor \
  workflow_parity::tests::modeled_workflow_replays_against_real_project_and_runtime_services \
  -- --ignored
```

See DIFFERENTIAL_HARNESS.md, UPSTREAM_BASELINE.md, and PUBLIC_RELEASE.md for comparison and release procedures. A skipped test is not a pass; a version number is not evidence of acceptance.
