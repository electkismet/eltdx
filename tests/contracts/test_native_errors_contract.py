"""Static contract for native error and ABI boundary behavior."""

from __future__ import annotations

import ast
from pathlib import Path

import pytest


ROOT = Path(__file__).parents[2]
ABI = ROOT / "src" / "eltdx" / "_native_abi.py"
ADAPTER = ROOT / "src" / "eltdx" / "transport" / "native.py"


def _source(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def test_native_abi_is_explicit_and_fail_fast() -> None:
    source = _source(ABI)
    tree = ast.parse(source)
    assignments = {
        node.targets[0].id: node.value.value
        for node in tree.body
        if isinstance(node, ast.Assign)
        and len(node.targets) == 1
        and isinstance(node.targets[0], ast.Name)
        and isinstance(node.value, ast.Constant)
    }
    assert assignments["EXPECTED_NATIVE_ABI_VERSION"] == 1
    assert "require_native_abi(native)" in source
    assert "raise ImportError" in source


def test_native_error_mapping_covers_every_structured_kind() -> None:
    source = _source(ADAPTER)
    string_literals = {
        node.value
        for node in ast.walk(ast.parse(source, filename=str(ADAPTER)))
        if isinstance(node, ast.Constant) and isinstance(node.value, str)
    }
    for kind in (
        "InvalidArgument",
        "Protocol",
        "ConnectionClosed",
        "Timeout",
        "PoolBusy",
        "PushOverflow",
        "CloseTimeout",
        "UnsupportedCommand",
        "Internal",
    ):
        assert kind in string_literals
    for exception_name in (
        "ProtocolError",
        "ConnectionClosedError",
        "ResponseTimeoutError",
        "PoolBusyError",
        "PushOverflowError",
        "TransportCloseTimeoutError",
        "UnsupportedCommandError",
        "TransportError",
    ):
        assert exception_name in source
    assert "_native_context" in source
    assert "raise mapped from None" in source
    assert "raise mapped from error.__cause__" in source


class NativeError(Exception):
    """Test double matching the private PyO3 exception name and payload."""


def _raise_native(error: BaseException) -> None:
    raise error


def test_native_error_without_cause_does_not_expose_private_wrapper() -> None:
    from eltdx.exceptions import ProtocolError
    from eltdx.transport.native import call_native

    private = NativeError(
        "Protocol",
        "invalid response",
        [("context", "test"), ("code", "invalid_data")],
    )
    with pytest.raises(ProtocolError) as raised:
        call_native(_raise_native, private)

    assert raised.value.__cause__ is None
    assert raised.value.__suppress_context__ is True


def test_native_error_preserves_genuine_existing_cause() -> None:
    from eltdx.exceptions import ProtocolError
    from eltdx.transport.native import call_native

    cleanup_error = RuntimeError("cleanup failed")
    private = NativeError(
        "Protocol",
        "invalid response",
        [("context", "test"), ("code", "invalid_data")],
    )
    private.__cause__ = cleanup_error
    with pytest.raises(ProtocolError) as raised:
        call_native(_raise_native, private)

    assert raised.value.__cause__ is cleanup_error
    assert raised.value.__suppress_context__ is True


def test_native_adapter_does_not_translate_unrelated_exceptions() -> None:
    source = _source(ADAPTER)
    assert 'if error.__class__.__name__ != "NativeError":' in source
    assert "raise\n" in source
    assert "json" not in source.lower()
