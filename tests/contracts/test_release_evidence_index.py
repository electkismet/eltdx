"""Contracts for the candidate-bound release evidence index."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from scripts.verification import index_release_evidence


CANDIDATE = "a" * 40


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _evidence_root(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    root = tmp_path / "artifacts" / "release-evidence" / CANDIDATE
    root.mkdir(parents=True)
    external = {}
    for key in sorted(index_release_evidence.EXPECTED_EXTERNAL_EVIDENCE):
        evidence = root / f"{key}.json"
        evidence.write_text(f'{{"key":"{key}"}}\n', encoding="utf-8")
        external[key] = {
            "path": str(evidence),
            "sha256": _sha256(evidence),
        }
    state = {
        "candidate": CANDIDATE,
        "failure": None,
        "completed_rounds": list(range(1, 9)),
        "active_round": 9,
        "round_progress": {
            "9": {
                "completed_steps": index_release_evidence.EXPECTED_FINAL_ROUND_STEPS,
            }
        },
        "external_evidence": external,
    }
    (root / "state.json").write_text(
        json.dumps(state),
        encoding="utf-8",
    )
    monkeypatch.setattr(index_release_evidence, "ROOT", tmp_path)
    monkeypatch.setattr(
        index_release_evidence.subprocess,
        "run",
        lambda *args, **kwargs: SimpleNamespace(stdout=f"{CANDIDATE}\n"),
    )
    return root


def test_index_excludes_mutable_round_log(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    root = _evidence_root(tmp_path, monkeypatch)
    ordinary = root / "round-01-source-and-compile" / "01-cargo-fmt.log"
    ordinary.parent.mkdir()
    ordinary.write_text("passed\n", encoding="utf-8")
    self_log = (
        root
        / "round-09-documentation-and-release"
        / "06-evidence-index.log"
    )
    self_log.parent.mkdir()
    self_log.write_text("mutable\n", encoding="utf-8")
    index = index_release_evidence.build_index(CANDIDATE, root)

    indexed_paths = {record["path"] for record in index["files"]}
    assert ordinary.relative_to(root).as_posix() in indexed_paths
    assert self_log.relative_to(root).as_posix() not in indexed_paths
    assert index["excluded_execution_roots"] == []
    assert (
        "round-09-documentation-and-release/06-evidence-index.log"
        in index["excluded_mutable_paths"]
    )


def test_index_still_rejects_unexpected_evidence_symlinks(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    root = _evidence_root(tmp_path, monkeypatch)
    target = root / "ordinary.log"
    target.write_text("passed\n", encoding="utf-8")
    unexpected = root / "unexpected.log"
    unexpected.symlink_to(target.name)

    with pytest.raises(
        ValueError,
        match=r"release evidence must not contain symlinks: unexpected\.log",
    ):
        index_release_evidence.build_index(CANDIDATE, root)
