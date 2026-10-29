import contextlib
import hashlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import check_public_history as history
import check_public_source as source


class PublicationChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rustplc-publication-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "docs").mkdir()
        (self.root / "docs/vendored-sha256.json").write_text("{}")

    def git(self, *args):
        return subprocess.run(
            ["git", "-C", str(self.root), "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args],
            check=True, capture_output=True,
        ).stdout

    def init(self):
        self.git("init", "--quiet")
        self.git("config", "user.name", "RustPLC synthetic test")
        self.git("config", "user.email", "tests@users.noreply.github.com")

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "--quiet", "-m", message)

    def run_source(self):
        with patch.object(source, "ROOT", self.root), contextlib.redirect_stdout(io.StringIO()):
            source.main()

    def home_path(self):
        return "/" + "home" + "/synthetic/project"

    def test_force_tracked_build_file_is_rejected(self):
        self.init()
        (self.root / ".gitignore").write_text("target/\n")
        (self.root / "target").mkdir()
        (self.root / "target/review.txt").write_text(self.home_path())
        self.git("add", "-f", "target/review.txt")
        with self.assertRaisesRegex(SystemExit, "non-publishable path"):
            self.run_source()

    def test_untracked_ignored_build_output_is_not_published_source(self):
        self.init()
        (self.root / ".gitignore").write_text("target/\n")
        (self.root / "target").mkdir()
        (self.root / "target/review.txt").write_text(self.home_path())
        self.run_source()
        self.assertNotIn(self.root / "target/review.txt", source.source_paths(self.root))

    def test_source_archive_cannot_hide_files_in_build_folders(self):
        (self.root / "artifacts").mkdir()
        (self.root / "artifacts/review.txt").write_text("synthetic")
        with self.assertRaisesRegex(SystemExit, "non-publishable path"):
            self.run_source()

    def test_deleted_historical_file_is_still_checked(self):
        self.init()
        (self.root / "review.txt").write_text(self.home_path())
        self.commit("test: add synthetic review fixture")
        (self.root / "review.txt").unlink()
        self.commit("test: remove synthetic review fixture")
        failures, commits, _ = history.history_failures(self.root, ["--all"])
        self.assertEqual(commits, 2)
        self.assertTrue(any("machine home path" in item for item in failures))
        self.assertTrue(all(self.home_path() not in item for item in failures))

    def test_personal_commit_email_is_rejected_without_logging_it(self):
        self.init()
        self.git("config", "user.email", "synthetic@example.test")
        self.commit("test: synthetic identity")
        failures, _, _ = history.history_failures(self.root, ["--all"])
        self.assertTrue(any("noreply identity" in item for item in failures))
        self.assertTrue(all("synthetic@example.test" not in item for item in failures))

    def test_public_noreply_history_passes(self):
        self.init()
        self.commit("test: clean source")
        failures, commits, _ = history.history_failures(self.root, ["--all"])
        self.assertEqual((failures, commits), ([], 1))

    def test_personal_email_in_commit_message_is_rejected(self):
        self.init()
        self.commit("test: synthetic contact synthetic@example.test")
        failures, _, _ = history.history_failures(self.root, ["--all"])
        self.assertTrue(any("metadata email" in item for item in failures))
        self.assertTrue(all("synthetic@example.test" not in item for item in failures))

    def test_github_bot_public_service_trailer_passes(self):
        self.init()
        self.commit("test: bot attribution\n\nSigned-off-by: dependabot[bot] <support@github.com>")
        failures, _, _ = history.history_failures(self.root, ["--all"])
        self.assertEqual(failures, [])

    def test_binary_invalid_encoding_and_private_repository_suffixes(self):
        self.assertTrue(source.check_blob("data.txt", b"\0"))
        self.assertTrue(source.check_blob("data.txt", b"\xff"))
        for suffix in ("-private", "_private", ".private", "2"):
            self.assertTrue(source.check_blob("data.txt", ("logohere/" + "rustplc" + suffix).encode()))
        self.assertEqual(source.check_blob("data.txt", b"https://github.com/logohere/rustplc.git"), [])

    def test_vendor_tampering_is_rejected(self):
        (self.root / "vendor.txt").write_text("changed")
        expected = hashlib.sha256(b"original").hexdigest()
        (self.root / "docs/vendored-sha256.json").write_text(json.dumps({"vendor.txt": expected}))
        with self.assertRaisesRegex(SystemExit, "vendored bytes changed"):
            self.run_source()


if __name__ == "__main__":
    unittest.main()
