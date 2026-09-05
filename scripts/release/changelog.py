import datetime
import pathlib
import re
import sys
import tomllib


UNRELEASED_HEADING = "\n## [Unreleased]\n"


def main():
    mode, tag, directory = sys.argv[1:]
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag):
        raise SystemExit("Expected a stable version tag such as v0.1.0")
    version = tag[1:]
    temporary = pathlib.Path(directory)
    changelog = pathlib.Path("CHANGELOG.md")
    text = changelog.read_text()
    if mode == "prepare":
        package = tomllib.loads(pathlib.Path("Cargo.toml").read_text())["package"]
        if package["version"] != version:
            raise SystemExit("Release tag must match the Cargo.toml version")
        if text.count(UNRELEASED_HEADING) != 1:
            raise SystemExit("Expected exactly one [Unreleased] section")
        if f"\n## [{version}]" in text:
            raise SystemExit("Version already exists in CHANGELOG.md")
        notes = text.split(UNRELEASED_HEADING, 1)[1].split("\n## [", 1)[0].strip()
        if not notes or not any(line.startswith("- ") for line in notes.splitlines()):
            raise SystemExit("The [Unreleased] section must contain release notes")
        temporary.joinpath("changelog-before.md").write_text(text)
        temporary.joinpath("release-notes.md").write_text(notes + "\n")
    elif mode == "finalize":
        if text != temporary.joinpath("changelog-before.md").read_text():
            raise SystemExit("Changelog changed since the release tag; reconcile it manually")
        date = datetime.datetime.now(datetime.timezone.utc).date().isoformat()
        heading = f"{UNRELEASED_HEADING}\n## [{version}] - {date}\n"
        changelog.write_text(text.replace(UNRELEASED_HEADING, heading, 1))
    else:
        raise SystemExit(f"Unknown operation: {mode}")


if __name__ == "__main__":
    main()
