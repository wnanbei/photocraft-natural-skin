#!/usr/bin/env python3
"""Native tag validation and draft-first GitHub publication (CI tooling only)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess
import tomllib
import zipfile

PREFIX = "native-v"
PLATFORMS = ("macOS-arm64", "Windows")
SEMVER = re.compile(
    r"(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)"
    r"(?:-(?P<pre>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?\Z"
)


class ReleaseError(Exception):
    pass


def parse_version(version):
    match = SEMVER.fullmatch(version)
    if not match or any(
        item.isdigit() and len(item) > 1 and item.startswith("0")
        for item in (match["pre"] or "").split(".")
    ):
        raise ReleaseError(f"Invalid semantic version: {version}")
    return tuple(int(match[name]) for name in ("major", "minor", "patch")), bool(match["pre"])


def version_from_tag(tag):
    if not tag.startswith(PREFIX):
        raise ReleaseError("Only native-v<version> tags can publish native software")
    version = tag[len(PREFIX):]
    parse_version(version)
    return version


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def changelog_notes(changelog, version):
    heading = re.search(rf"^## Native {re.escape(version)}(?: — [^\n]+)?\s*$", changelog, re.M)
    if not heading:
        raise ReleaseError(f"CHANGELOG.md has no Native {version} section")
    following = changelog[heading.end():]
    body = re.split(r"^## ", following, maxsplit=1, flags=re.M)[0].strip()
    if not body:
        raise ReleaseError(f"CHANGELOG.md has no notes for {version}")
    return body


def source(tag):
    version = version_from_tag(tag)
    try:
        commit = git("rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
        read = lambda path: git("show", f"{commit}:{path}")
        manifest = tomllib.loads(read("native/Cargo.toml"))
        locked = tomllib.loads(read("Cargo.lock"))
        notes = changelog_notes(read("CHANGELOG.md"), version)
    except (subprocess.CalledProcessError, tomllib.TOMLDecodeError) as exc:
        raise ReleaseError(f"Cannot read the existing native tag {tag}") from exc
    if manifest["package"]["version"] != version:
        raise ReleaseError("Tag version does not match native/Cargo.toml")
    packages = [p for p in locked["package"] if p["name"] == "natural-skin-companion"]
    if len(packages) != 1 or packages[0]["version"] != version:
        raise ReleaseError("Cargo.lock does not match the tagged application version")
    return {"tag": tag, "version": version, "commit": commit, "notes": notes,
            "prerelease": str(parse_version(version)[1]).lower()}


def archive_names(version):
    return [f"Natural-Skin-{version}-{platform}.zip" for platform in PLATFORMS]


def check_archive(path, version, platform):
    try:
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            if len(names) != len(set(names)):
                raise ReleaseError(f"Duplicate archive entries: {path.name}")
            if archive.testzip() is not None:
                raise ReleaseError(f"Corrupt archive: {path.name}")
            if platform == "Windows":
                if not {"natural-skin.exe", "natural-skin-companion.exe"}.issubset(names):
                    raise ReleaseError("Windows package is missing its executables")
            else:
                bundle = "Natural Skin.app/Contents/"
                if bundle + "MacOS/natural-skin" not in names:
                    raise ReleaseError("macOS package is missing the application")
                info = plistlib.loads(archive.read(bundle + "Info.plist"))
                base = ".".join(str(n) for n in parse_version(version)[0])
                if info.get("CFBundleShortVersionString") != base:
                    raise ReleaseError("macOS bundle version differs from the release tag")
    except (zipfile.BadZipFile, KeyError, plistlib.InvalidFileException) as exc:
        raise ReleaseError(f"Invalid release archive: {path.name}") from exc


def assemble(directory, metadata):
    expected = archive_names(metadata["version"])
    actual = sorted(path.name for path in directory.glob("*.zip"))
    if actual != sorted(expected):
        raise ReleaseError(f"Expected all supported platform packages: {expected}; found: {actual}")
    checksums = []
    for name, platform in zip(expected, PLATFORMS):
        path = directory / name
        if path.is_symlink() or not path.is_file():
            raise ReleaseError(f"Not a regular release package: {name}")
        check_archive(path, metadata["version"], platform)
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        checksums.append(f"{digest}  {name}\n")
    (directory / "SHA256SUMS.txt").write_text("".join(checksums), encoding="utf-8")
    notes = (
        f"# Natural Skin Native {metadata['version']}\n\n{metadata['notes']}\n\n"
        "## Downloads\n\n"
        "- **macOS Apple Silicon:** choose the `macOS-arm64.zip` package.\n"
        "- **Windows x64:** choose the `Windows.zip` portable package and run `natural-skin.exe`.\n\n"
        "Verify downloaded ZIPs against `SHA256SUMS.txt`. Release assets stay attached to this "
        "version; the temporary Actions artifact retention does not apply to them.\n\n"
        "macOS bundles use an ad-hoc development signature, not Apple notarization. "
        "Windows packages are portable builds without a signed installer. "
        "CI build/test success does not establish real-photo, calibrated-monitor or native "
        "Windows interaction acceptance.\n\n"
        f"Source tag: `{metadata['tag']}`\n\nSource commit: `{metadata['commit']}`\n"
    )
    (directory / "RELEASE_NOTES.md").write_text(notes, encoding="utf-8")
    return [directory / name for name in expected] + [directory / "SHA256SUMS.txt"]


def gh(*args, required=True):
    result = subprocess.run(["gh", *args], capture_output=True, text=True, check=False)
    if required and result.returncode:
        raise ReleaseError(f"gh {' '.join(args[:2])} failed: {result.stderr.strip()}")
    return result


def is_latest(version, published_tags):
    numbers, prerelease = parse_version(version)
    if prerelease:
        return False
    for tag in published_tags:
        try:
            other, pre = parse_version(version_from_tag(tag))
        except ReleaseError:
            continue
        if not pre and other > numbers:
            return False
    return True


def ensure_draft(metadata, repository):
    remote = gh("api", f"repos/{repository}/commits/{metadata['tag']}", "--jq", ".sha").stdout.strip()
    if remote != metadata["commit"]:
        raise ReleaseError("The remote tag moved during the build; refusing to publish")
    view = gh("release", "view", metadata["tag"], "--repo", repository,
              "--json", "isDraft,url", required=False)
    if not view.returncode:
        existing = json.loads(view.stdout)
        if not existing["isDraft"]:
            print(f"Already published; existing release left unchanged: {existing['url']}")
            return True
    elif "HTTP 404" not in view.stderr and "release not found" not in view.stderr.lower():
        raise ReleaseError(f"Cannot inspect the release: {view.stderr.strip()}")
    else:
        gh("release", "create", metadata["tag"], "--repo", repository, "--verify-tag",
           "--draft", "--title", f"Natural Skin Native {metadata['version']}",
           "--notes", metadata["notes"],
           f"--prerelease={metadata['prerelease']}", "--latest=false")
    return False


def publish(directory, metadata, repository):
    assets = assemble(directory, metadata)
    if ensure_draft(metadata, repository):
        return
    # Clobber is restricted to drafts. A failed upload leaves an unpublished draft
    # which can be retried; published software is never replaced by a rerun.
    gh("release", "upload", metadata["tag"], "--repo", repository,
       "--clobber", *(str(path) for path in assets))
    tags = gh("api", f"repos/{repository}/releases?per_page=100", "--paginate", "--jq",
              ".[] | select(.draft == false) | .tag_name").stdout.splitlines()
    gh("release", "edit", metadata["tag"], "--repo", repository, "--verify-tag",
       "--title", f"Natural Skin Native {metadata['version']}",
       "--notes-file", str(directory / "RELEASE_NOTES.md"),
       f"--prerelease={metadata['prerelease']}",
       f"--latest={str(is_latest(metadata['version'], tags)).lower()}", "--draft=false")
    print(gh("release", "view", metadata["tag"], "--repo", repository, "--json", "url", "--jq", ".url").stdout.strip())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "draft", "assemble", "publish"])
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", help="Expected immutable source commit")
    parser.add_argument("--directory", type=Path, default=Path("release"))
    parser.add_argument("--output", type=Path, help="GitHub Actions output file")
    parser.add_argument("--download", action="store_true", help="Download platform ZIPs from the draft before publication")
    args = parser.parse_args()
    try:
        metadata = source(args.tag)
        if args.commit and args.commit != metadata["commit"]:
            raise ReleaseError("The source tag differs from the validated commit")
        repository = os.environ.get("GITHUB_REPOSITORY", "")
        if args.command in ("draft", "publish") and not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
            raise ReleaseError("GITHUB_REPOSITORY must identify the publishing repository")
        if args.command in ("prepare", "draft"):
            if args.command == "draft":
                metadata["published"] = str(ensure_draft(metadata, repository)).lower()
            if args.output:
                with args.output.open("a", encoding="utf-8") as stream:
                    for key in ("tag", "version", "commit", "prerelease", "published"):
                        if key not in metadata:
                            continue
                        stream.write(f"{key}={metadata[key]}\n")
            print(json.dumps({key: value for key, value in metadata.items() if key != "notes"}))
        elif args.command == "assemble":
            assemble(args.directory, metadata)
        else:
            if args.download:
                if ensure_draft(metadata, repository):
                    return
                args.directory.mkdir(parents=True, exist_ok=True)
                gh("release", "download", metadata["tag"], "--repo", repository,
                   "--pattern", f"Natural-Skin-{metadata['version']}-*.zip", "--dir", str(args.directory))
            publish(args.directory, metadata, repository)
    except (ReleaseError, OSError, ValueError) as exc:
        parser.exit(1, f"Release blocked: {exc}\n")


if __name__ == "__main__":
    main()
