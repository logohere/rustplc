# Security policy

Report vulnerabilities through [private vulnerability reporting](https://github.com/logohere/rustplc/security/advisories/new), not public issues. If the private form is unavailable, do not post sensitive details publicly; that channel must be enabled before reports are accepted.

Include the affected commit/version, minimal isolated reproduction, expected/observed behavior, impact and suggested fix. Use synthetic projects and sanitized logs. Do not test on physical equipment or third-party systems.

Reports against `development` and the latest published release are accepted. The maintainer will assess, coordinate a regression/fix, and agree on disclosure. No deadline, bounty, certification or guaranteed remediation is promised.

RustPLC is not safety-certified. Security fixes and simulator tests do not qualify it for physical control. Repository policy is not proof of live settings: verify controls before publication and releases. See docs/MAINTAINERS.md.

## Dependency checks

The dependency-security workflow runs Cargo Audit on PRs, integration pushes, and daily. Vulnerability records fail the job; informational maintenance and unsoundness warnings remain visible and require review. No advisories are ignored.

The preparation audit still reports `glib` 0.18.5 and `rand` 0.7.3 unsoundness warnings in the cross-platform lockfile, plus unmaintained `fxhash` and `proc-macro-error`. `glib` and `rand` 0.7.3 are absent from the checked macOS dependency graph; that does not establish other platforms as safe. Their framework dependency paths require a separately tested migration. Linux/Windows editor acceptance is not established. The legacy Cocoa `block` dependency also has a future-Rust compatibility warning. Do not interpret a successful audit as proof that these warnings or all security risks are resolved.
