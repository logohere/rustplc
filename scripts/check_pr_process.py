#!/usr/bin/env python3
import json
import os
import re
import sys

from github_delivery import CONFIG, acceptance_complete, acceptance_prerequisites, candidate_tree, get, primary_issue, validate_pr


def main() -> None:
    event = json.loads(open(os.environ["GITHUB_EVENT_PATH"]).read())
    kind = os.environ["GITHUB_EVENT_NAME"]
    merged = kind == "push"
    if kind == "pull_request":
        pr = get(f"pulls/{event['number']}")
    elif kind == "workflow_dispatch":
        value = event.get("inputs", {}).get("pr_number", "")
        if not re.fullmatch(r"[1-9][0-9]*", value):
            raise ValueError("dispatch requires a positive PR number")
        pr = get(f"pulls/{value}")
        merged = bool(pr.get("merged"))
    elif kind == "push":
        sha = os.environ["GITHUB_SHA"]
        if not re.fullmatch(r"[0-9a-f]{40}", sha):
            raise ValueError("invalid source revision")
        target = os.environ["GITHUB_REF_NAME"]
        matches = [pr for pr in get(f"commits/{sha}/pulls") if pr.get("merge_commit_sha") == sha and pr["base"]["ref"] == target]
        if len(matches) != 1:
            raise ValueError("integration commit needs one associated merged PR")
        pr = get(f"pulls/{matches[0]['number']}")
        if not pr.get("merged"):
            raise ValueError("integration PR is not merged")
    else:
        raise ValueError("unsupported process event")
    issue = get(f"issues/{primary_issue(pr)}")
    validate_pr(pr, issue, merged)
    if kind == "push":
        parents = get(f"git/commits/{os.environ['GITHUB_SHA']}")["parents"]
        synchronization = pr["base"]["ref"] == CONFIG["development_branch"] and pr["head"]["ref"] == CONFIG["release_branch"]
        if len(parents) != (2 if synchronization else 1):
            raise ValueError("incorrect integration strategy; squash normal/promotional PRs and merge release ancestry synchronization")
    if pr["base"]["ref"] == CONFIG["release_branch"]:
        acceptance_complete(issue, merged)
        acceptance_prerequisites()
        candidate_tree(issue, pr["head"]["sha"])
    print(f"Process checks: PR #{pr['number']} references issue #{issue['number']}; target and delivery definitions verified.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError) as error:
        sys.exit(str(error))
