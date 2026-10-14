"""Check out released source for release-plz's API comparison."""

import argparse
from pathlib import Path
import subprocess


def repair_dependencies(directory: Path, tag: str) -> None:
    if tag != "v0.32.1":
        return

    # These requirements now admit incompatible APIs. Use the versions recorded
    # in this release's Cargo.lock without changing its Rust source.
    manifest = directory / "Cargo.toml"
    contents = manifest.read_text()
    for name, requirement, released_version in [
        ("async-openai", ">=0.29.6", "0.30.1"),
        ("qdrant-client", "1.15", "1.15.0"),
        ("pgvector", "0.4", "0.4.1"),
        ("spider", "2.37", "2.37.159"),
    ]:
        original = f'{name} = {{ version = "{requirement}"'
        replacement = f'{name} = {{ version = "={released_version}"'
        if contents.count(original) != 1:
            raise ValueError(f"Unexpected {name} requirement in v0.32.1")
        contents = contents.replace(original, replacement)
    manifest.write_text(contents)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    subprocess.run(
        ["git", "clone", "--quiet", "--no-hardlinks", "--no-checkout", ".", str(args.destination)],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(args.destination), "checkout", "--quiet", "--detach", args.tag],
        check=True,
    )
    repair_dependencies(args.destination, args.tag)


if __name__ == "__main__":
    main()
