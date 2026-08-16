"""Check release notes and publish workflow retain explicit authorization gates."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def check() -> list[str]:
    errors: list[str] = []
    release = (ROOT / "docs" / "releases" / "v3.0.1.md").read_text(encoding="utf-8")
    changelog = (ROOT / "docs" / "CHANGELOG.md").read_text(encoding="utf-8")
    publish = (ROOT / ".github" / "workflows" / "publish.yml").read_text(encoding="utf-8")
    required_release = (
        "## 主要变化",
        "## 数据口径",
        "## 安装和平台",
        "## 升级注意",
        "## 验证和发布状态",
        "五个独立 ABI3 wheel",
        "一个",
        "同一批产物",
    )
    for needle in required_release:
        if needle not in release:
            errors.append(f"release notes missing {needle!r}")
    if "## v3.0.1 - 2026-08-17" not in changelog:
        errors.append("changelog does not contain the dated v3.0.1 release")
    if "待发布" in release or "预发布候选" in release:
        errors.append("release notes still describe a pending prerelease")
    for needle in ("name: pypi", "refs/tags/v"):
        if needle not in publish:
            errors.append(f"publish workflow missing gate {needle!r}")
    forbidden = ("TODO", "FIXME", "TBD", "PLACEHOLDER")
    for label, text in (("release notes", release), ("changelog", changelog)):
        for marker in forbidden:
            if marker in text:
                errors.append(f"{label} contains unresolved marker {marker}")
    return errors


def main() -> int:
    errors = check()
    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
