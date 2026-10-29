# Maintainer rules

The owner controls access, repository rules, Actions, and releases. Contributor status grants no write, maintain, or admin access. Use least privilege; review access, apps, keys, and automation before releases. The requester cannot approve their own access or ownership change. Never share credentials.

Use proper issues, scoped branches, verified PRs, and REVIEW_GUIDELINES.md. Changes to dependencies, workflows, licenses, security controls, or maintainer policy require explicit owner review. No automatic merge of bot or agent PRs. Require passing checks, resolved conversations, and fresh approval after new commits.

With a sole maintainer, GitHub cannot provide independent approval of that maintainer's own PR. An owner-only approval bypass must be PR-only, recorded in the PR, and never bypass status checks or allow direct pushes. Do not describe self-review as independent review.

Workflow tokens default to read-only. Fork code receives no secrets or publishing credentials. Never execute untrusted code under privileged `pull_request_target` or `workflow_run`. Pin third-party Actions to full commit hashes; require approval for outside-contributor runs. Keep automatic PR approval and auto-merge disabled.

Release through a reviewed `development` to `main` PR and protected version tag. Verify applicable heavy/manual tests, hashes, privacy, provenance, notices, recovery, and supported platforms. Binaries need a separate audit for machine paths and signing metadata. Do not publish logs, screenshots, user projects, or credentials.

Repository controls cannot prevent an owner account compromise. Use phishing-resistant two-factor authentication and protected recovery methods. Repository configuration does not verify those account settings.
