# Contributing

## Where to contribute

RustPLC is maintained separately from OpenPLC and STruC++. Open RustPLC issues and PRs here; the pinned upstreams define comparison targets, not automatic synchronization or shared maintainership. A RustPLC-only failure is not evidence of an upstream bug.

For a proposed upstream fix, reproduce it against that upstream, read its current contribution/licensing rules, and prepare the exact issue or PR for the owner's manual review before an agent posts anything there. RustPLC's contribution policy does not replace another project's policy or CLA.

## Research and issues first

Read AGENTS.md, SECURITY.md, docs/ENGINEERING_RULES.md, docs/UPSTREAM_BASELINE.md, docs/REVIEW_GUIDELINES.md, and the affected implementation and tests before editing.

Use an existing issue for the same failure. Otherwise use the bug form and include exact versions, a minimal reproducer, expected/observed behavior, impact, and runnable acceptance checks. Discuss features, dependencies, baseline updates, and broad refactors with the maintainer before implementation. No duplicate issues or bulk unsolicited PRs.

## Workflow

1. Branch from `development`: `bugfix/gh-<issue>-<slug>` or `feature/gh-<issue>-<slug>`.
2. Keep one issue and coherent change per branch. Preserve unrelated work.
3. Add a focused regression. Run the README checks and applicable differential, end-to-end, and manual tests. Record failures and skips; a skip is not a pass.
4. Use concise Conventional Commits explaining why. No credentials, private links, machine paths, or conversation logs.
   Use a GitHub public noreply email for commit authorship and attribution trailers; private contact addresses and machine identities must not enter public Git history.
5. Open a PR against `development`, link the issue, complete the template, and disclose substantial AI assistance.
6. Obtain maintainer review and rerun checks after changes. Release PRs promote `development` to `main`. Never push directly or force-push protected branches.

Compiler/runtime changes need a generated-program or protocol end-to-end test. UI changes need rendered interaction checks. Record actual manual test steps and platform gaps; macOS simulation does not prove Windows/Linux packaging or hardware behavior.

## Code and licensing

Preserve architecture, typed errors, recovery, input bounds, and reviewed ABI/OS boundaries. Keep allocation, blocking and logging out of real-time paths. Avoid unrelated cleanup and duplicate abstractions.

New dependencies require a rationale, source, version, license, and maintenance assessment. Contributions use the affected component's existing license. Preserve upstream notices and pins; do not submit code you lack permission to contribute. There is no separate RustPLC CLA or permission to relicense third-party work.

## AI and conduct

Contributors must understand and review AI-assisted work. Human review is required before agents post issues, comments, commits, pushes, or PRs to third-party repositories. Never invent test evidence.

Follow CODE_OF_CONDUCT.md. Send vulnerabilities through SECURITY.md, not public issues.
