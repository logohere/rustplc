import copy
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import audit_release_artifacts as artifacts
import check_release as release
from github_delivery import CONFIG, acceptance_complete, acceptance_prerequisites, primary_issue, validate_pr

SHA = "a" * 40
TREE = "b" * 40
DIGEST = "c" * 64


def issue():
    return {"number": 24, "state": "open", "milestone": {"number": 1}, "labels": [],
            "body": "## Goal\nImplement scoped delivery controls.\n## Acceptance\n- [ ] Run the defined checks.\n"}


def pull_request():
    repo = {"full_name": CONFIG["repository"]}
    return {"number": 29, "user": {"id": 69400476}, "base": {"ref": "development", "repo": repo},
            "head": {"ref": "feature/gh-24-delivery-pipeline", "repo": repo, "sha": SHA},
            "body": "## Issue\nCloses #24.\n## Verification\nLocal synthetic process checks pass; hosted checks pending.\n"}


def release_issue():
    value = issue()
    value["labels"] = [{"name": "release"}]
    value["body"] = (f"Source tree: {TREE}\nArtifact SHA-256: {DIGEST}\n"
                     "## Acceptance\n- [x] Source reviewed\n- [x] Checks passed\n- [x] Privacy checked\n- [x] Risks reviewed\n"
                     "## Known risks\nSynthetic test; no artifact is distributed.\n"
                     "## Publication acceptance\n- [x] Downloaded\n- [x] Interactions checked\n- [x] Notices checked\n- [x] Publication approved\n")
    return value


class ProcessChecks(unittest.TestCase):
    def test_scoped_issue_and_branch_pass(self):
        validate_pr(pull_request(), issue())

    def test_wrong_branch_closed_issue_missing_milestone_and_empty_acceptance_fail(self):
        for change in ("branch", "closed", "milestone", "acceptance"):
            pr, ticket = pull_request(), issue()
            if change == "branch": pr["head"]["ref"] = "feature/gh-25-unrelated"
            if change == "closed": ticket["state"] = "closed"
            if change == "milestone": ticket["milestone"] = None
            if change == "acceptance": ticket["body"] = "## Acceptance\n"
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_pr(pr, ticket)

    def test_code_block_does_not_supply_issue_directive(self):
        pr = pull_request()
        pr["body"] = "```\nCloses #24\n```\n## Verification\nLocal checks pass."
        with self.assertRaises(ValueError): primary_issue(pr)

    def test_dependency_bot_exception_requires_official_identity_and_repository(self):
        pr, ticket = pull_request(), issue()
        pr["user"]["id"] = 49699333
        pr["head"]["ref"] = "dependabot/cargo/example-1.2.3"
        pr["body"] = "Dependency update; owner review remains required."
        ticket["number"] = CONFIG["dependency_tracking_issue"]
        validate_pr(pr, ticket)
        pr["user"]["id"] = 1
        with self.assertRaises(ValueError): primary_issue(pr)

    def test_main_rejects_feature_branch_and_fork_promotion(self):
        pr, ticket = pull_request(), release_issue()
        pr["base"]["ref"] = "main"
        pr["body"] = "Refs #24.\n## Verification\nRelease candidate checks recorded."
        with self.assertRaises(ValueError): validate_pr(pr, ticket)
        pr["head"]["ref"] = "development"
        validate_pr(pr, ticket)
        pr["head"]["repo"] = {"full_name": "example/fork"}
        with self.assertRaises(ValueError): validate_pr(pr, ticket)

    def test_main_ancestry_sync_requires_release_issue(self):
        pr, ticket = pull_request(), issue()
        pr["head"]["ref"] = "main"
        with self.assertRaises(ValueError): validate_pr(pr, ticket)
        ticket["labels"] = [{"name": "release"}]
        validate_pr(pr, ticket)
        pr["head"]["repo"] = {"full_name": "example/fork"}
        with self.assertRaises(ValueError): validate_pr(pr, ticket)

    def test_release_acceptance_and_completed_prerequisite_are_required(self):
        ticket = release_issue()
        acceptance_complete(ticket)
        ticket["body"] = ticket["body"].replace("[x] Source", "[ ] Source")
        with self.assertRaises(ValueError): acceptance_complete(ticket)
        with self.assertRaises(ValueError): acceptance_prerequisites(lambda _: {"state": "closed", "state_reason": "not_planned"})
        acceptance_prerequisites(lambda _: {"state": "closed", "state_reason": "completed"})


class ReleaseChecks(unittest.TestCase):
    def setUp(self):
        self.responses = {
            "git/ref/heads/main": {"object": {"sha": SHA}},
            "git/ref/tags/v1.0.0": {"object": {"type": "tag", "sha": "d" * 40}},
            "git/tags/" + "d" * 40: {"object": {"type": "commit", "sha": SHA}, "tagger": {"email": "tests@users.noreply.github.com"}},
            "git/commits/" + SHA: {"tree": {"sha": TREE}},
            "issues/24": release_issue(),
            "issues/20": {"state": "closed", "state_reason": "completed"},
            f"commits/{SHA}/check-runs?filter=latest&per_page=100": {"check_runs": [
                {"name": name, "status": "completed", "conclusion": "success", "app": {"id": 15368}}
                for name in CONFIG["required_checks"]]},
        }

    def validate(self, **kwargs):
        release.release_eligible("v1.0.0", 24, SHA, "refs/heads/main", self.responses.__getitem__, **kwargs)

    def test_complete_metadata_can_be_validated_without_publishing(self):
        self.validate()
        self.validate(artifact=DIGEST)

    def test_stale_main_wrong_tag_tree_open_prerequisite_and_failed_check_fail(self):
        for change in ("main", "tag", "tree", "prerequisite", "check"):
            with self.subTest(change=change):
                original = copy.deepcopy(self.responses)
                if change == "main": self.responses["git/ref/heads/main"]["object"]["sha"] = "e" * 40
                if change == "tag": self.responses["git/ref/tags/v1.0.0"]["object"]["type"] = "commit"
                if change == "tree": self.responses["git/commits/" + SHA]["tree"]["sha"] = "e" * 40
                if change == "prerequisite": self.responses["issues/20"]["state"] = "open"
                if change == "check": self.responses[f"commits/{SHA}/check-runs?filter=latest&per_page=100"]["check_runs"][0]["conclusion"] = "failure"
                with self.assertRaises(ValueError): self.validate()
                self.responses = original

    def test_other_repository_and_unreviewed_artifact_fail(self):
        with self.assertRaises(ValueError): self.validate(repository="example/fork")
        with self.assertRaises(ValueError): self.validate(artifact="e" * 64)
        self.responses["issues/24"]["body"] = self.responses["issues/24"]["body"].replace("[x] Downloaded", "[ ] Downloaded")
        with self.assertRaises(ValueError): self.validate(artifact=DIGEST)


class ArtifactChecks(unittest.TestCase):
    def test_binary_utf8_and_utf16_home_paths_are_detected_without_printing_values(self):
        private = "/" + "home" + "/synthetic/project"
        for encoding in ("utf-8", "utf-16-le", "utf-16-be"):
            with self.subTest(encoding=encoding), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                (root / "binary").write_bytes(private.encode(encoding))
                failures = artifacts.audit(root)
                self.assertTrue(any("machine home path" in item for item in failures))
                self.assertTrue(all(private not in item for item in failures))

    def test_non_sensitive_binary_bytes_are_not_a_source_file_violation(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "binary").write_bytes(b"\x7fELF\0synthetic")
            self.assertEqual(artifacts.audit(root), [])


if __name__ == "__main__":
    unittest.main()
