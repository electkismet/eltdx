"""Verify the first installed native import and fail-fast ABI behavior."""

from __future__ import annotations

import importlib.metadata
from types import ModuleType


def main() -> int:
    import eltdx
    from eltdx import _native
    from eltdx._native_abi import (
        EXPECTED_NATIVE_ABI_VERSION,
        load_native,
        require_native_abi,
    )

    installed_version = importlib.metadata.version("eltdx")
    if eltdx.__version__ != installed_version:
        raise AssertionError(
            f"package version mismatch: module={eltdx.__version__!r}, "
            f"metadata={installed_version!r}"
        )
    if load_native() is not _native:
        raise AssertionError("ABI loader did not return the installed native module")
    if _native.ABI_VERSION != EXPECTED_NATIVE_ABI_VERSION:
        raise AssertionError(
            f"native ABI mismatch: native={_native.ABI_VERSION!r}, "
            f"expected={EXPECTED_NATIVE_ABI_VERSION!r}"
        )

    mismatched = ModuleType("eltdx._native_mismatch_probe")
    mismatched.ABI_VERSION = EXPECTED_NATIVE_ABI_VERSION + 1
    try:
        require_native_abi(mismatched)
    except ImportError as error:
        message = str(error)
        if "native ABI mismatch" not in message or "expected=" not in message:
            raise AssertionError(f"ABI mismatch error lost its context: {message!r}") from error
    else:
        raise AssertionError("a mismatched native ABI was accepted")

    print(f"eltdx={installed_version} native_abi={_native.ABI_VERSION}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
