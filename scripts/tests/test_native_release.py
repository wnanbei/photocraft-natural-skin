import hashlib
import importlib.util
import json
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location("native_release", Path(__file__).parents[1] / "native_release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def metadata(self, version="0.3.0"):
        return {"tag": f"native-v{version}", "version": version, "commit": "a" * 40,
                "notes": "- A tested change.", "prerelease": str(release.parse_version(version)[1]).lower()}

    def packages(self, directory, version="0.3.0"):
        for platform, name in zip(release.PLATFORMS, release.archive_names(version)):
            with zipfile.ZipFile(directory / name, "w") as archive:
                if platform == "Windows":
                    archive.writestr("natural-skin.exe", b"gui fixture")
                    archive.writestr("natural-skin-companion.exe", b"cli fixture")
                else:
                    base = ".".join(str(n) for n in release.parse_version(version)[0])
                    archive.writestr("Natural Skin.app/Contents/Info.plist", plistlib.dumps({"CFBundleShortVersionString": base}))
                    archive.writestr("Natural Skin.app/Contents/MacOS/natural-skin", b"gui fixture")

    def result(self, stdout="", stderr="", returncode=0):
        return subprocess.CompletedProcess([], returncode, stdout, stderr)

    def test_native_semver_tags_and_prereleases(self):
        self.assertEqual(release.version_from_tag("native-v0.3.1"), "0.3.1")
        self.assertEqual(release.parse_version("0.4.0-rc.1+build.2"), ((0, 4, 0), True))
        for tag in ["v1.0.0", "main", "native-v01.2.3", "native-v1.2", "native-v1.2.3-rc.01", "native-v1.2.3\n", "native-v1.2.3;echo hi"]:
            with self.subTest(tag=tag), self.assertRaises(release.ReleaseError):
                release.version_from_tag(tag)

    def test_changelog_extracts_only_the_exact_version(self):
        notes = "# Changes\n\n## Native 0.3.0 — 2026-11-11\n\n- Current.\n\n## Native 0.2.0 — 2026-11-09\n\n- Old.\n"
        self.assertEqual(release.changelog_notes(notes, "0.3.0"), "- Current.")
        for version in ["0.3", "0.4.0"]:
            with self.assertRaises(release.ReleaseError):
                release.changelog_notes(notes, version)
        with self.assertRaises(release.ReleaseError):
            release.changelog_notes("## Native 0.3.0\n\n## Other\n", "0.3.0")

    def test_tag_manifest_and_lockfile_must_agree(self):
        files = {
            "native/Cargo.toml": '[package]\nversion="0.3.0"',
            "Cargo.lock": '[[package]]\nname="natural-skin-companion"\nversion="0.3.0"',
            "CHANGELOG.md": "## Native 0.3.0 — 2026-11-11\n\n- Current.",
        }
        def fake_git(*args):
            return "a" * 40 if args[0] == "rev-parse" else files[args[1].split(":", 1)[1]]
        with patch.object(release, "git", side_effect=fake_git):
            self.assertEqual(release.source("native-v0.3.0")["commit"], "a" * 40)
            files["native/Cargo.toml"] = '[package]\nversion="0.2.0"'
            with self.assertRaisesRegex(release.ReleaseError, "manifest|native/Cargo"):
                release.source("native-v0.3.0")
            files["native/Cargo.toml"] = '[package]\nversion="0.3.0"'
            files["Cargo.lock"] = files["Cargo.lock"].replace("0.3.0", "0.2.0")
            with self.assertRaisesRegex(release.ReleaseError, "Cargo.lock"):
                release.source("native-v0.3.0")

    def test_all_platform_packages_are_required_before_assembly(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory)
            (directory / release.archive_names("0.3.0")[1]).unlink()
            with self.assertRaisesRegex(release.ReleaseError, "all supported"):
                release.assemble(directory, self.metadata())
            self.assertFalse((directory / "SHA256SUMS.txt").exists())

    def test_bundle_version_and_real_zip_contents_are_checked(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory, "0.2.0")
            old = directory / release.archive_names("0.2.0")[0]
            path = directory / release.archive_names("0.3.0")[0]
            old.rename(path)
            with self.assertRaisesRegex(release.ReleaseError, "bundle version"):
                release.check_archive(path, "0.3.0", "macOS-arm64")
            path.write_bytes(b"Not a zip")
            with self.assertRaises(release.ReleaseError):
                release.check_archive(path, "0.3.0", "macOS-arm64")
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr("README.txt", "missing executables")
            with self.assertRaisesRegex(release.ReleaseError, "executables"):
                release.check_archive(path, "0.3.0", "Windows")

    def test_checksums_and_notes_describe_the_exact_source_and_archives(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory)
            assets = release.assemble(directory, self.metadata())
            self.assertEqual({asset.name for asset in assets}, {
                "Natural-Skin-0.3.0-macOS-arm64.zip",
                "Natural-Skin-0.3.0-Windows.zip",
                "SHA256SUMS.txt",
            })
            for line in (directory / "SHA256SUMS.txt").read_text().splitlines():
                digest, name = line.split("  ")
                self.assertEqual(digest, hashlib.sha256((directory / name).read_bytes()).hexdigest())
            notes = (directory / "RELEASE_NOTES.md").read_text()
            self.assertIn("a" * 40, notes)
            self.assertIn("native-v0.3.0", notes)
            self.assertIn("not Apple notarization", notes)
            self.assertNotIn("Intel", notes)
            self.assertNotIn("macOS-x86_64", notes)

    def test_prereleases_and_older_backfills_do_not_replace_latest(self):
        self.assertFalse(release.is_latest("0.4.0-rc.1", []))
        self.assertFalse(release.is_latest("0.3.0", ["native-v0.4.0"]))
        self.assertTrue(release.is_latest("0.3.0", ["v1.0.0", "native-v0.2.0", "native-v0.4.0-rc.1"]))

    def test_publish_uploads_to_a_draft_before_making_it_public(self):
        calls = []
        def fake_gh(*args, **kwargs):
            calls.append(args)
            if args[:1] == ("api",):
                return self.result("a" * 40 if "/commits/" in args[1] else "v1.0.0\n")
            if args[:2] == ("release", "view"):
                return self.result(stderr="release not found", returncode=1) if len(calls) == 2 else self.result("https://example.test/release")
            return self.result()
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory)
            with patch.object(release, "gh", side_effect=fake_gh):
                release.publish(directory, self.metadata(), "owner/repo")
        creation = next(c for c in calls if c[:2] == ("release", "create"))
        self.assertIn("--draft", creation)
        self.assertIn("--verify-tag", creation)
        upload = next(i for i, c in enumerate(calls) if c[:2] == ("release", "upload"))
        public = next(i for i, c in enumerate(calls) if "--draft=false" in c)
        self.assertLess(upload, public)

    def test_failed_upload_never_publishes_a_partial_release(self):
        calls = []
        def fake_gh(*args, **kwargs):
            calls.append(args)
            if args[:1] == ("api",):
                return self.result("a" * 40)
            if args[:2] == ("release", "view"):
                return self.result(stderr="release not found", returncode=1)
            if args[:2] == ("release", "upload"):
                raise release.ReleaseError("Upload failed")
            return self.result()
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory)
            with patch.object(release, "gh", side_effect=fake_gh), self.assertRaises(release.ReleaseError):
                release.publish(directory, self.metadata(), "owner/repo")
        self.assertFalse(any("--draft=false" in call for call in calls))

    def test_published_releases_are_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory)
            with patch.object(release, "gh", side_effect=[self.result("a" * 40), self.result(json.dumps({"isDraft": False, "url": "https://example.test/published"}))]) as mock:
                release.publish(directory, self.metadata(), "owner/repo")
            self.assertEqual(mock.call_count, 2)

    def test_existing_draft_is_resumed_without_creating_another_release(self):
        with patch.object(release, "gh", side_effect=[
            self.result("a" * 40),
            self.result(json.dumps({"isDraft": True, "url": "https://example.test/draft"})),
        ]) as mock:
            self.assertFalse(release.ensure_draft(self.metadata(), "owner/repo"))
        self.assertEqual(mock.call_count, 2)

    def test_moved_remote_tag_blocks_all_release_writes(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory)
            with patch.object(release, "gh", return_value=self.result("b" * 40)) as mock:
                with self.assertRaisesRegex(release.ReleaseError, "tag moved"):
                    release.publish(directory, self.metadata(), "owner/repo")
            self.assertEqual(mock.call_count, 1)

    def test_network_or_permission_errors_are_not_mistaken_for_a_new_release(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            self.packages(directory)
            with patch.object(release, "gh", side_effect=[self.result("a" * 40), self.result(stderr="HTTP 403", returncode=1)]) as mock:
                with self.assertRaisesRegex(release.ReleaseError, "Cannot inspect"):
                    release.publish(directory, self.metadata(), "owner/repo")
            self.assertEqual(mock.call_count, 2)


if __name__ == "__main__":
    unittest.main()
