# Agent instructions

- Read CONTRIBUTING.md, SECURITY.md, docs/MAINTAINERS.md and docs/REVIEW_GUIDELINES.md before editing.
- Use a proper existing issue or reviewed new report, scoped branch, tests and reviewed PR; never push directly to main/development.
- Do not post issues, comments, commits, pushes or PRs to third-party repositories before the owner's manual review of the exact contribution.
- Do not publish source, change visibility, grant access or release artifacts without explicit owner authorization; source publication does not authorize a binary release.
- Exclude private projects, credentials, local usernames/home paths and profile data. Preserve required third-party attribution and the intentional README creator credit.

- This is a compatibility port, not a toy PLC and not a safety-rated controller.
- Read `docs/PORT_PLAN.md` before changing runtime behavior.
- Follow `docs/ENGINEERING_RULES.md`, including Definition of Ready and Definition of Done.
- Pin and record the OpenPLC baseline for every parity claim.
- Add a differential fixture before marking behavior verified.
- Do not enable real hardware or network writes in tests.
- Keep changes small; preserve upstream notices for adapted code.
