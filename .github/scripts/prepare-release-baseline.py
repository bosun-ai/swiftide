"""Check out released source for release-plz's API comparison."""

import argparse
from pathlib import Path
import subprocess


def repair_dependencies(directory: Path, tag: str) -> None:
    if tag != "v0.32.1":
        return

    # The published requirement admits incompatible OpenAI APIs. Use the version
    # recorded in this release's Cargo.lock without changing its Rust source.
    manifest = directory / "Cargo.toml"
    original = 'async-openai = { version = ">=0.29.6", default-features = false }'
    replacement = 'async-openai = { version = "=0.30.1", default-features = false }'
    contents = manifest.read_text()
    if contents.count(original) != 1:
        raise ValueError("Unexpected async-openai requirement in v0.32.1")
    manifest.write_text(contents.replace(original, replacement))


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
