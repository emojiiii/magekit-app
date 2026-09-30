"""Release regressions use real Git histories and never call the GitHub API."""

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location("release", Path(__file__).parents[1] / "release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release@example.invalid")
        self.commit("Initial source", {"Cargo.toml": '[workspace.package]\nversion = "0.1.0"\n', "src/main.rs": "original\n"})
        self.git("tag", "v0.1.0")
        self.statuses = {"v0.1.0": "published"}
        for patch in (
            mock.patch.object(release, "ROOT", self.root),
            mock.patch.object(release, "release_status", side_effect=lambda tag: self.statuses.get(tag, "missing")),
            mock.patch.dict(os.environ, {"GITHUB_REF": "refs/heads/main", "GITHUB_SHA": ""}),
        ):
            patch.start()
            self.addCleanup(patch.stop)

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, stderr=subprocess.STDOUT, text=True).strip()

    def commit(self, message, files):
        for name, content in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        self.git("add", ".")
        self.git("commit", "-m", message)
        return self.git("rev-parse", "HEAD")

    def plan(self):
        with mock.patch.object(release, "append_outputs") as outputs:
            release.plan_release()
            return outputs.call_args.kwargs

    def test_docs_only_does_not_release(self):
        self.commit("docs: guide [skip release]", {"README.md": "guide\n"})
        self.assertFalse(self.plan()["release"])

    def test_docs_head_cannot_hide_pending_feature(self):
        self.commit("feat: new UI", {"src/main.rs": "new UI\n"})
        self.commit("docs: screenshots [skip release]", {"README.md": "guide\n"})
        self.assertEqual(self.plan(), {"release": True, "version": "v0.2.0", "previous_tag": "v0.1.0"})

    def test_docs_head_without_marker_preserves_backlog(self):
        self.commit("fix: startup", {"src/main.rs": "fixed\n"})
        self.commit("docs: guide", {"README.md": "guide\n"})
        self.assertEqual(self.plan()["version"], "v0.1.1")

    def test_skip_marker_applies_to_own_code_commit(self):
        self.commit("chore: deferred [release skip]", {"src/main.rs": "deferred\n"})
        self.assertFalse(self.plan()["release"])
        self.commit("docs: guide", {"README.md": "guide\n"})
        self.assertFalse(self.plan()["release"])

    def test_skipped_code_head_cannot_hide_prior_eligible_code(self):
        self.commit("fix: startup", {"src/main.rs": "fixed\n"})
        self.commit("chore: deferred [skip release]", {"src/other.rs": "later\n"})
        self.assertEqual(self.plan()["version"], "v0.1.1")

    def test_reverted_code_with_only_net_docs_change_does_not_release(self):
        self.commit("fix: temporary", {"src/main.rs": "temporary\n"})
        self.commit("revert: restore", {"src/main.rs": "original\n", "README.md": "guide\n"})
        self.assertFalse(self.plan()["release"])

    def test_merge_backlog_survives_skipped_docs_head(self):
        self.git("checkout", "-b", "feature")
        self.commit("feat: nested change", {"src/main.rs": "feature\n"})
        self.git("checkout", "main")
        self.git("merge", "--no-ff", "feature", "-m", "Merge feature")
        self.commit("docs: guide [skip release]", {"README.md": "guide\n"})
        self.assertEqual(self.plan()["version"], "v0.2.0")

    def test_skipped_merge_does_not_trigger_release(self):
        self.git("checkout", "-b", "feature")
        self.commit("feat: deferred", {"src/main.rs": "feature\n"})
        self.git("checkout", "main")
        self.git("merge", "--no-ff", "feature", "-m", "Merge feature [skip release]")
        self.assertFalse(self.plan()["release"])

    def test_existing_missing_tag_can_resume_only_same_source(self):
        self.commit("fix: startup", {"src/main.rs": "fixed\n"})
        self.git("tag", "v0.1.1")
        self.assertEqual(self.plan()["version"], "v0.1.1")
        self.commit("docs: guide [skip release]", {"README.md": "guide\n"})
        self.assertEqual(self.plan()["version"], "v0.1.2")

    def test_published_head_is_not_republished(self):
        self.assertFalse(self.plan()["release"])

    def test_mismatched_workflow_sha_fails_closed(self):
        with mock.patch.dict(os.environ, {"GITHUB_SHA": "0" * 40}):
            with self.assertRaises(release.ReleaseError):
                self.plan()

    def test_publication_accepts_same_source(self):
        head = self.git("rev-parse", "HEAD")
        self.assertTrue(release.can_publish(head, head))

    def test_publication_accepts_docs_only_advance_without_retargeting_tag(self):
        head = self.git("rev-parse", "HEAD")
        main = self.commit("docs: guide [skip release]", {"docs/guide.md": "guide\n", "README.md": "guide\n"})
        self.assertTrue(release.can_publish(head, main))
        self.assertEqual(self.git("rev-parse", "v0.1.0"), head)

    def test_publication_defers_for_new_source(self):
        head = self.git("rev-parse", "HEAD")
        main = self.commit("fix: newer source", {"src/main.rs": "new\n"})
        self.assertFalse(release.can_publish(head, main))

    def test_publication_defers_for_build_configuration(self):
        head = self.git("rev-parse", "HEAD")
        main = self.commit("ci: workflow", {".github/workflows/release.yml": "changed\n"})
        self.assertFalse(release.can_publish(head, main))

    def test_publication_rejects_rewritten_main_history(self):
        head = self.commit("fix: built source", {"src/main.rs": "built\n"})
        self.git("checkout", "-b", "diverged", "v0.1.0")
        main = self.commit("docs: unrelated history", {"README.md": "guide\n"})
        with self.assertRaises(release.ReleaseError):
            release.can_publish(head, main)


if __name__ == "__main__":
    unittest.main()
