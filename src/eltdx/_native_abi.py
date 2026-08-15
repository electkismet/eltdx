"""Private ABI compatibility checks for the Rust extension."""

from __future__ import annotations

import importlib
from types import ModuleType


EXPECTED_NATIVE_ABI_VERSION = 1


def load_native() -> ModuleType:
    """Load ``eltdx._native`` and reject a mismatched Python/native pair."""

    try:
        native = importlib.import_module("eltdx._native")
    except ImportError as exc:
        raise ImportError(
            "eltdx Rust extension is unavailable; install a native wheel or build the sdist"
        ) from exc
    return require_native_abi(native)


def require_native_abi(native: ModuleType) -> ModuleType:
    """Validate the native module's private ABI before exposing any engine."""

    actual = getattr(native, "ABI_VERSION", None)
    if actual != EXPECTED_NATIVE_ABI_VERSION:
        raise ImportError(
            "eltdx native ABI mismatch: "
            f"native={actual!r}, expected={EXPECTED_NATIVE_ABI_VERSION}"
        )
    return native


__all__ = ["EXPECTED_NATIVE_ABI_VERSION", "load_native", "require_native_abi"]
