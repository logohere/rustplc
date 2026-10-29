#!/usr/bin/env python3
import argparse
import os
import re
import sys

from check_public_history import PUBLIC_EMAIL
from github_delivery import CONFIG, ROOT, acceptance_complete, acceptance_prerequisites, candidate_tree, get, section


def verify_source(tag: str, sha: str, ref: str, fetch, repository: str) -> None:
    if repository != CONFIG["repository"]:
        raise ValueError("unexpected release repository")
    if ref != "refs/heads/" + CONFIG["release_branch"] or not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("release validation must run from an exact main revision")
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*)?", tag):
        raise ValueError("invalid release tag")
    version = re.search(r'(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"$', (ROOT / "Cargo.toml").read_text())[1]
    if tag[1:].split("-", 1)[0] != version:
        raise ValueError("tag version does not match the workspace")
    if fetch("git/ref/heads/" + CONFIG["release_branch"])["object"]["sha"] != sha:
        raise ValueError("main advanced; review the current revision before release")
    reference = fetch("git/ref/tags/" + tag)["object"]
    if reference["type"] != "tag":
        raise ValueError("release requires an owner-created annotated tag")
    annotated = fetch("git/tags/" + reference["sha"])
    if annotated["object"]["type"] != "commit" or annotated["object"]["sha"] != sha:
        raise ValueError("release tag must name the reviewed main commit")
    if not PUBLIC_EMAIL.fullmatch(annotated["tagger"].get("email", "")):
        raise ValueError("tagger requires a public-safe identity")


def verify_issue(number: int, sha: str, fetch, artifact: str) -> None:
    issue = fetch(f"issues/{number}")
    acceptance_complete(issue)
    if not issue.get("milestone"):
        raise ValueError("release needs a milestone")
    section(issue.get("body") or "", "Known risks")
    candidate_tree(issue, sha, fetch)
    acceptance_prerequisites(fetch)
    if artifact:
        if not re.fullmatch(r"[0-9a-f]{64}", artifact):
            raise ValueError("invalid candidate artifact digest")
        matches = re.findall(r"(?m)^Artifact SHA-256: ([0-9a-f]{64})$", issue.get("body") or "")
        if matches != [artifact]:
            raise ValueError("release issue must identify the reviewed candidate artifact")
        publication = section(issue.get("body") or "", "Publication acceptance")
        if re.search(r"(?im)^\s*-\s+\[ \]", publication) or len(re.findall(r"(?im)^\s*-\s+\[[xX]\]", publication)) < 4:
            raise ValueError("candidate publication acceptance is incomplete")


def verify_checks(sha: str, fetch) -> None:
    checks = fetch(f"commits/{sha}/check-runs?filter=latest&per_page=100")["check_runs"]
    for name in CONFIG["required_checks"]:
        matches = [check for check in checks if check["name"] == name and check.get("app", {}).get("id") == 15368]
        if len(matches) != 1 or matches[0]["status"] != "completed" or matches[0]["conclusion"] != "success":
            raise ValueError(f"required check is not successful: {name}")


def release_eligible(tag: str, number: int, sha: str, ref: str, fetch=get, artifact: str = "", repository: str = CONFIG["repository"]) -> None:
    if number < 1:
        raise ValueError("invalid release issue number")
    verify_source(tag, sha, ref, fetch, repository)
    verify_issue(number, sha, fetch, artifact)
    verify_checks(sha, fetch)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--issue", required=True, type=int)
    parser.add_argument("--artifact-sha", default="")
    args = parser.parse_args()
    sha = os.environ["GITHUB_SHA"]
    release_eligible(args.tag, args.issue, sha, os.environ["GITHUB_REF"], artifact=args.artifact_sha, repository=os.environ["GITHUB_REPOSITORY"])
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a") as handle:
            handle.write(f"sha={sha}\ntag={args.tag}\nissue={args.issue}\n")
    print(f"Release eligibility verified for {args.tag} at {sha}; no artifact published.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError) as error:
        sys.exit(str(error))
