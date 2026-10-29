# RustPLC versioning policy

RustPLC uses its own [Semantic Versioning](https://semver.org/) lifecycle. OpenPLC version numbers are compatibility baselines, not RustPLC product versions.

## Current line

RustPLC `1.0.0` is the first accepted macOS production release against the pinned compatibility baseline below.

The pinned compatibility baseline is recorded separately:

- OpenPLC Editor `4.2.10` at commit `32e324c7c8278cbf95e8b519fb6b935312ffcfc4`
- OpenPLC Runtime v4 at commit `bf82b1b661fd95c9899969f629d692b77b4e1454`
- STruC++ v0.6.2 ABI baseline / `e48bc707dcce00572c07b07160a4437a5e5aed45`

## Release rules

- `0.x.y`: pre-1.0 parity/development releases. Compatibility can still change as the OpenPLC contract is completed.
- `1.0.0`: first professional macOS release for which the pinned-baseline ledger has no release-blocking unapproved divergence and all required compiler/runtime, UI, packaging, MCP, security, persistence, accessibility, and release gates pass.
- `1.x.y`: backwards-compatible RustPLC releases after 1.0.
  - patch: bug/security fixes with no intentional public compatibility break;
  - minor: backwards-compatible features or support expansion;
  - major: intentional incompatible RustPLC public/project/runtime/API contract change.
- Platform support expansion does not copy an OpenPLC version number. A Windows/Linux support addition after 1.0 is a minor release unless it requires an incompatible public contract.

## Compatibility metadata

Release notes must state the OpenPLC Editor/Runtime/STruC++ pins used for differential evidence. These pins are compatibility metadata, not SemVer ordering.

## Tagging

Stable releases use annotated tags of the form `vMAJOR.MINOR.PATCH`, for example `v1.0.0`. Pre-release candidates use SemVer suffixes such as `v1.0.0-rc.1`.

`v1.0.0` is scoped to the documented pins. It does not imply safety certification, Windows/Linux product acceptance, or parity with OpenPLC changes made after those pins. No build script or CI job may infer compatibility from the version alone.
