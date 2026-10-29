import json
import os
import re
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONFIG = json.loads((ROOT / ".github/delivery.json").read_text())


def get(path: str):
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not token:
        raise ValueError("a read-scoped GitHub token is required")
    request = urllib.request.Request(
        "https://api.github.com/repos/" + CONFIG["repository"] + "/" + path,
        headers={"Authorization": "Bearer " + token, "Accept": "application/vnd.github+json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except (urllib.error.URLError, json.JSONDecodeError) as error:
        raise ValueError("GitHub metadata could not be verified; refusing delivery") from error


def primary_issue(pr: dict) -> int:
    body = re.sub(r"(?s)```.*?```", "", pr.get("body") or "")
    if pr.get("user", {}).get("id") == 49699333:
        if not pr["head"]["ref"].startswith("dependabot/") or pr["head"]["repo"]["full_name"] != CONFIG["repository"]:
            raise ValueError("dependency automation must use its repository-owned branch")
        return CONFIG["dependency_tracking_issue"]
    verb = "Refs" if pr["base"]["ref"] == CONFIG["release_branch"] else "(?:Closes|Fixes|Resolves)"
    matches = re.findall(rf"(?im)^\s*{verb}\s+#([1-9][0-9]*)\s*\.?\s*$", body)
    if len(matches) != 1:
        raise ValueError("provide exactly one primary issue directive on its own line")
    return int(matches[0])


def section(body: str, title: str) -> str:
    match = re.search(rf"(?im)^#{{2,3}}\s+{title}\s*$", body)
    if not match:
        raise ValueError(f"missing {title} section")
    content = re.split(r"(?m)^#{2,3}\s+", body[match.end():], maxsplit=1)[0].strip()
    if not content or content == "_No response_":
        raise ValueError(f"empty {title} section")
    return content


def validate_pr(pr: dict, issue: dict, merged: bool = False) -> None:
    if pr["base"]["repo"]["full_name"] != CONFIG["repository"]:
        raise ValueError("unexpected target repository")
    target = pr["base"]["ref"]
    if target not in {CONFIG["development_branch"], CONFIG["release_branch"]}:
        raise ValueError("PR must target development or main")
    number = primary_issue(pr)
    if issue.get("number") != number or "pull_request" in issue:
        raise ValueError("primary reference must resolve to a real issue")
    if issue.get("state") != "open" and not merged:
        raise ValueError("implementation issue must remain open until delivery")
    if not issue.get("milestone"):
        raise ValueError("assign the issue to a delivery milestone")
    body = issue.get("body") or ""
    section(body, "Acceptance(?: checks)?")
    if pr.get("user", {}).get("id") == 49699333:
        return
    section(pr.get("body") or "", "Verification")
    if target == CONFIG["release_branch"]:
        if pr["head"]["ref"] != CONFIG["development_branch"] or pr["head"]["repo"]["full_name"] != CONFIG["repository"]:
            raise ValueError("main accepts only repository-owned development promotions")
        if "release" not in {label["name"] for label in issue.get("labels", [])}:
            raise ValueError("main promotion needs a release issue")
    elif pr["head"]["ref"] == CONFIG["release_branch"]:
        if pr["head"]["repo"]["full_name"] != CONFIG["repository"] or "release" not in {label["name"] for label in issue.get("labels", [])}:
            raise ValueError("release ancestry synchronization needs an owned main branch and release issue")
    elif not re.fullmatch(rf"(?:bugfix|feature|chore)/gh-{number}-[a-z0-9]+(?:-[a-z0-9]+)*", pr["head"]["ref"]):
        raise ValueError("branch name must match the primary issue")


def acceptance_complete(issue: dict, merged: bool = False) -> None:
    completed = merged and issue.get("state") == "closed" and issue.get("state_reason") == "completed"
    if "pull_request" in issue or (issue.get("state") != "open" and not completed):
        raise ValueError("release issue must be an open issue")
    if "release" not in {label["name"] for label in issue.get("labels", [])}:
        raise ValueError("release issue requires the release label")
    body = section(issue.get("body") or "", "Acceptance(?: checks)?")
    if re.search(r"(?im)^\s*-\s+\[ \]", body) or len(re.findall(r"(?im)^\s*-\s+\[[xX]\]", body)) < 4:
        raise ValueError("release acceptance must be explicitly completed with evidence")


def acceptance_prerequisites(fetch=get) -> None:
    for number in CONFIG["release_acceptance_issues"]:
        issue = fetch(f"issues/{number}")
        if issue.get("state") != "closed" or issue.get("state_reason") != "completed":
            raise ValueError(f"release prerequisite #{number} is not completed")


def candidate_tree(issue: dict, sha: str, fetch=get) -> None:
    expected = re.findall(r"(?m)^Source tree: ([0-9a-f]{40})$", issue.get("body") or "")
    if len(expected) != 1 or fetch(f"git/commits/{sha}")["tree"]["sha"] != expected[0]:
        raise ValueError("release evidence must identify the exact candidate source tree")
