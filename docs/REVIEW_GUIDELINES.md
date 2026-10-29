# Review guidelines

- Proper linked issue, smallest coherent scope, pinned authority, and explicit acceptance checks.
- Existing architecture and recovery preserved; no fake controls, speculative abstractions, or unsupported claims.
- Regression demonstrates the original failure. Unit, differential, end-to-end and manual evidence is real; skips and failures remain visible.
- Review ABI layout, ownership, concurrency, scan boundaries, FFI unwind containment, input bounds, and real-time behavior together.
- Dependencies and vendored files retain provenance, license text, notices, and component license boundaries.
- No private projects, credentials, local usernames/home paths, machine profiles, or unrelated account links in source, fixtures, logs, or artifacts. The README creator credit is intentional.
- UI changes include rendered interaction/error checks, not just a successful build.
- Workflow changes keep untrusted code unprivileged, use read-only default tokens, and pin Actions. Access/policy/release changes need owner review.
- Documentation reflects actual behavior and gaps. Do not infer safety, production, platform or security assurances from passing checks.
- Required checks, review conversations, and human approval are satisfied before merge. No third-party contribution is posted before owner review.
