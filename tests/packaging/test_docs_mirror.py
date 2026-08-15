"""Contracts for the generated package documentation mirror."""

from __future__ import annotations

from pathlib import Path

import pytest

from scripts.package.sync_docs import DESTINATION, SOURCE, differences, sync_docs


def test_packaged_docs_match_authoritative_root() -> None:
    assert differences(SOURCE, DESTINATION) == ()


def test_sync_docs_copies_and_removes_only_bounded_files(tmp_path: Path) -> None:
    source = tmp_path / "source"
    destination = tmp_path / "destination"
    (source / "nested").mkdir(parents=True)
    destination.mkdir()
    (source / "a.md").write_text("a\n", encoding="utf-8")
    (source / "nested" / "b.md").write_text("b\n", encoding="utf-8")
    (destination / "stale.md").write_text("stale\n", encoding="utf-8")

    first = sync_docs(source, destination, check=False, max_changes=2)
    assert len(first.changed) == 2
    assert len(first.remaining) == 1
    second = sync_docs(source, destination, check=False, max_changes=2)
    assert len(second.changed) == 1
    assert second.remaining == ()
    assert differences(source, destination) == ()


def test_check_mode_never_changes_destination(tmp_path: Path) -> None:
    source = tmp_path / "source"
    destination = tmp_path / "destination"
    source.mkdir()
    destination.mkdir()
    (source / "index.md").write_text("new\n", encoding="utf-8")
    (destination / "index.md").write_text("old\n", encoding="utf-8")

    result = sync_docs(source, destination, check=True, max_changes=None)
    assert result.changed == ()
    assert result.remaining == ("index.md",)
    assert (destination / "index.md").read_text(encoding="utf-8") == "old\n"


def test_sync_rejects_destination_inside_source(tmp_path: Path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    with pytest.raises(ValueError, match="must not be inside"):
        sync_docs(source, source / "mirror", check=False, max_changes=None)
