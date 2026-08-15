"""Create GitHub Release notes from the frozen version document."""

from __future__ import annotations

import argparse
import os
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TAG_PATTERN = re.compile(r"v[0-9]+\.[0-9]+\.[0-9]+(?:(?:a|b|rc)[0-9]+)?")
FORBIDDEN_MARKERS = ("TODO", "FIXME", "TBD", "PLACEHOLDER")


def prepare(tag: str, output: Path) -> None:
    if TAG_PATTERN.fullmatch(tag) is None:
        raise ValueError("tag must be a normalized v-prefixed PEP 440 release")
    source = ROOT / "docs" / "releases" / f"{tag}.md"
    if not source.is_file():
        raise FileNotFoundError(f"release notes do not exist for {tag}: {source}")
    text = source.read_text(encoding="utf-8")
    if not text.startswith(f"# {tag}\n"):
        raise ValueError(f"release notes heading does not match {tag}")
    unresolved = [marker for marker in FORBIDDEN_MARKERS if marker in text]
    if unresolved:
        raise ValueError(f"release notes contain unresolved markers: {unresolved!r}")
    output = output.resolve()
    if output == source.resolve():
        raise ValueError("release output must not overwrite the source document")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(f".{output.name}.tmp-{os.getpid()}")
    temporary.write_text(text, encoding="utf-8")
    temporary.replace(output)


def main() -> int:
    parser = argparse.ArgumentParser(description="Prepare frozen GitHub Release notes")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    prepare(args.tag, args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
