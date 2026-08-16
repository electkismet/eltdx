"""Freeze bytecode exclusion across build and release verification assets."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def test_maturin_excludes_bytecode_from_both_distribution_formats() -> None:
    pyproject = (ROOT / "pyproject.toml").read_text(encoding="utf-8")
    for pattern in ("**/__pycache__/**", "**/*.pyc", "**/*.pyo"):
        assert f'{{ path = "{pattern}", format = ["sdist", "wheel"] }}' in pyproject


def test_release_archives_reject_sdist_and_wheel_bytecode() -> None:
    sdist = (ROOT / "scripts/verification/verify_sdist.py").read_text(encoding="utf-8")
    wheels = (ROOT / "scripts/verification/verify_release_artifacts.py").read_text(
        encoding="utf-8"
    )
    installed = (ROOT / "scripts/verification/installed_smoke.py").read_text(encoding="utf-8")
    assert '"__pycache__" in path.parts' in sdist
    assert 'path.suffix in {".pyc", ".pyo"}' in sdist
    assert '"__pycache__" in member.parts' in wheels
    assert 'member.suffix in {".pyc", ".pyo"}' in wheels
    assert "importlib.metadata.files" not in installed
