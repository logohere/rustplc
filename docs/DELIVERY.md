# Delivery and release process

RustPLC is a desktop/runtime source project. Production delivery means a reviewed versioned artifact, not automatic control of physical equipment. A development merge is not a release.

## Work definitions and status

Use an existing matching issue or the bug, delivery-task, port-work-item or release form. Record the goal, current evidence, scope/exclusions, acceptance checks, dependencies, platform/privacy/license risks and rollback where applicable. Assign the repository owner and a milestone; do not invent other people's commitments or dates.

- `status: ready`: evidence and runnable acceptance checks exist.
- `status: in-progress`: implementation or research is active.
- `status: in-review`: an issue-linked PR is open; failures and gaps are visible.
- `status: blocked`: name the concrete dependency and next action.
- Closed/completed: acceptance was verified and the implementation merged. Remove the work-status label and record the PR, revision, checks, manual evidence and remaining exclusions.

Tracking issues stay open until their required children are complete. A completed development issue does not assert that its change reached a released binary. Release issues stay open through promotion/candidate review and close only after publication. Close duplicates or canceled work with the proper reason and replacement link, not as completed implementation.

## Development PRs

1. Branch from current `development`: `feature/gh-<issue>-<slug>`, `bugfix/gh-<issue>-<slug>` or maintenance `chore/gh-<issue>-<slug>`.
2. Keep one primary issue. Put `Closes #<issue>` on its own line in the PR body. Add other context with `Refs`; do not auto-close unresolved parent work.
3. Record exact verification and gaps under `## Verification`. Use focused regressions plus applicable native, differential and rendered checks. Preserve public-safe commit identity and required attribution.
4. Obtain review and pass `Rust checks`, `Public source checks`, `Dependency audit`, `Process checks` and `Portable checks` on the current head. Portable checks exercise the core/compiler on Linux/Windows, not acceptance of the desktop app there.
5. Squash-merge to `development`. GitHub closes the linked issue because `development` is the default branch. Update its acceptance evidence and milestone/status honestly.

Official repository-owned Dependabot PRs use the open dependency-review tracking issue configured in `.github/delivery.json`. They still need individual release-note/lockfile/compatibility review and all checks; no automatic approval or merge is granted. Replace the tracking issue when beginning a new maintenance cycle.

The Process check validates metadata and post-merge strategy. It does not prove issue claims are true or replace owner/code-owner review. Agents must not claim independent human approval. The documented sole-maintainer PR-only approval exception cannot bypass the separate required-check rules.

## Promotion and ancestry

`main` is the release line. Its initial administrative creation points to an already reviewed source revision; that bootstrap is not a version tag, binary release or production acceptance. No new feature is promoted by bypassing its normal PR.

For a release, open a release issue with completed promotion `Acceptance`, exact candidate tree identity, verification evidence and `Known risks`. The packaged-app prerequisite in `.github/delivery.json` must be closed as completed. Open `development` → `main`, put `Refs #<release-issue>` on its own line, and pass all required checks/review. Squash-merge the promotion; `main` remains linear.

After promotion, open a separate release-labeled synchronization issue and `main` → `development` PR with `Closes #<sync-issue>` and verification evidence. **Merge**, rather than squash, this synchronization PR so development contains the actual main commit before the next promotion. Normal feature and promotion PRs remain squash-only by procedure. Development permits merge commits for this necessary ancestry sync; the post-merge Process check rejects the wrong strategy and blocks release eligibility. Neither branch permits direct pushes, force-pushes, deletion or required-check bypass.

## Candidate and publication gates

1. Record `Source tree: <40-character tree ID>` on its own line in the release issue. Compare it with the actual candidate's `git rev-parse HEAD^{tree}`; commit IDs can change on a squash promotion while the accepted tree must not.
2. The owner creates an annotated `vMAJOR.MINOR.PATCH` or prerelease tag at the exact reviewed `main` revision using a public-safe tagger identity. Version must match the workspace. Version tags cannot be moved or deleted. Tags alone do not publish binaries.
3. Dispatch **Release pipeline** from `main`, supplying the existing tag and release issue. `validate_only` defaults to true: metadata and exact required GitHub Actions checks are validated without building/uploading anything.
4. A non-validation run first waits for explicit owner approval in `release-candidate`. It builds with the locked toolchain/dependencies, verifies bundle resources/codesign, scans artifact privacy patterns, and only then uploads the candidate ZIP and checksum for seven days. Artifacts from a public repository can be downloaded by other GitHub users; treat this approval as authorization to distribute the candidate. Never approve it before the relevant privacy/package prerequisites are complete.
5. Download and inspect the **actual CI-built candidate**. Record `Artifact SHA-256: <64-character digest>` and complete `Publication acceptance` in the release issue. Review UI/error/recovery behavior, signing scope, notices/corresponding source, known advisories and supported platforms. A source check or checksum alone does not establish these results.
6. Approve the `production` environment only for that artifact. The publish job rechecks the current main/tag/tree/checks/acceptance and matches the downloaded ZIP to the immutable build output and issue digest. Only that job receives contents/issues write permission. It publishes a release for the existing tag, then closes the release issue with the actual source/artifact identity. No build or fork code is executed in that write-enabled job beyond reviewed main's delivery validation.

Both environments are main-only and require the repository owner. Self-review prevention is false because there is one maintainer; owner approval is not independent approval. No agent automatically approves deployment gates. Administrators can alter repository/environment controls, so owner account security and access review remain necessary. Configuration files are inputs, not proof of live settings: read back reviewers and deployment branch restrictions before a release.

The release workflow is configured but not an assertion that a binary release has been exercised. Initial package/UI/privacy acceptance remains tracked separately. No release is run merely to close pipeline work.

## Rollback and incidents

Stop publication if any source, metadata, UI, privacy, dependency, signing or artifact check fails. Leave the release issue open with the exact failure and a scoped fix ticket. Do not dismiss warnings or fabricate manual evidence.

If a published artifact is defective, record the affected version and mitigation, direct users to a previously reviewed immutable release with its checksums, and produce a new reviewed patch release. Do not rewrite old tags or silently replace bytes. Removal of a published asset, signing changes or deployment to actual equipment needs a separate explicit decision; this workflow does not operate user machines or controllers.

Upstream refresh follows UPSTREAM_BASELINE.md and issue-scoped differential evidence. Third-party issues/PRs still require the owner's manual review. See PUBLIC_RELEASE.md and MAINTAINERS.md.
