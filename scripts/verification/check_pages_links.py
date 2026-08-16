"""Check built Pages HTML for broken local links and fragment targets."""

from __future__ import annotations

import argparse
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


class LinkParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.links: list[str] = []
        self.ids: set[str] = set()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        identity = values.get("id")
        if identity:
            self.ids.add(identity)
        for name in ("href", "src"):
            value = values.get(name)
            if value:
                self.links.append(value)


def _document(path: Path) -> LinkParser:
    parser = LinkParser()
    parser.feed(path.read_text(encoding="utf-8"))
    parser.close()
    return parser


def _target(site: Path, source: Path, raw_path: str) -> Path:
    decoded = unquote(raw_path)
    if decoded.startswith("/eltdx/"):
        decoded = decoded[len("/eltdx/") :]
        return site / decoded
    if decoded.startswith("/"):
        return site / decoded.lstrip("/")
    return source.parent / decoded


def check_site(site: Path) -> list[str]:
    errors: list[str] = []
    documents = {path.resolve(): _document(path) for path in site.rglob("*.html")}
    if not documents:
        return [f"no HTML files found under {site}"]
    for source, parsed in documents.items():
        for raw in parsed.links:
            split = urlsplit(raw)
            if split.scheme or split.netloc or raw.startswith(("mailto:", "tel:", "data:")):
                continue
            target = _target(site.resolve(), source, split.path)
            if not split.path:
                target = source
            if target.is_dir():
                target = target / "index.html"
            if not target.suffix:
                html_target = target.with_suffix(".html")
                target = html_target if html_target.is_file() else target / "index.html"
            target = target.resolve()
            if not target.is_file():
                errors.append(f"{source.relative_to(site.resolve())}: missing {raw}")
                continue
            if split.fragment and target.suffix == ".html":
                target_document = documents.get(target) or _document(target)
                fragment = unquote(split.fragment)
                if fragment not in target_document.ids:
                    errors.append(
                        f"{source.relative_to(site.resolve())}: missing fragment {raw}"
                    )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("site", type=Path)
    args = parser.parse_args()
    errors = check_site(args.site.resolve())
    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
