# Public release procedure

## Source

- Use only reviewed source from this repository. Do not import private history, issue threads, PR bodies, local profiles, build output or signing material.
- Run README checks, Gitleaks, and applicable differential, native, rendered-UI and manual checks. Record skipped checks honestly.
- Verify vendored hashes and preserve all component notices. Legacy OSCAT redistribution remains subject to its own free-of-charge and attribution terms.
- Source publication is not a binary release, notarization, hardware qualification or safety certification.

## Repository controls

After the owner approves publication, verify these settings through the GitHub API:

- `main` and `development`: PR-only changes, required GitHub Actions checks `Rust checks`, `Public source checks`, and `Dependency audit`, up-to-date branches, resolved conversations, linear history, no deletion or force-push. These gates have no bypass actors.
- A separate review ruleset requires fresh owner/code-owner approval; only the repository owner can bypass that approval on a PR. That exception does not bypass the mandatory check ruleset.
- `v*` tags: owner-only creation and separate no-bypass update/deletion restrictions.
- Actions: read-only default token, no PR-approval authority, full-SHA requirement, only the explicit action allowlist, all outside-contributor runs need approval.
- Auto-merge off, squash-only merges, obsolete branches deleted, wiki/projects/discussions off, no unneeded collaborators, deploy keys or publishing secrets.
- Private vulnerability reporting, dependency alerts, secret scanning and secret push protection enabled where GitHub supports them.

Ruleset requests are in `.github/rulesets/`. They are configuration inputs, not proof of enforcement. Private GitHub Free repositories cannot enforce these rulesets; apply and read them back after public visibility. Do not upgrade a paid plan as a substitute without owner authorization.

The initial repository import creates `development` from a clean, source-only commit before branch rules exist. Later changes require the normal issue/branch/PR process. `main` is reserved for a separately reviewed release promotion; bootstrap does not merge a release into it.

## Before tagging or distributing binaries

Run all applicable heavy tests, inspect the actual packaged app, review release metadata and machine-path/signing-identity exposure, include complete corresponding source and notices, verify hashes, and record supported platforms. Do not infer acceptance from a green fast suite.
