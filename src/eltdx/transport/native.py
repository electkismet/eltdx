"""Python boundary for the private Rust transport extension."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from types import ModuleType
from typing import Any, TypeVar

from eltdx._native_abi import load_native
from eltdx.exceptions import (
    ConnectionClosedError,
    PoolBusyError,
    ProtocolError,
    PushOverflowError,
    ResponseTimeoutError,
    TransportCloseTimeoutError,
    TransportError,
    UnsupportedCommandError,
)

T = TypeVar("T")

_INVALID_ARGUMENT_TYPES: dict[str, type[Exception]] = {
    "ValueError": ValueError,
    "TypeError": TypeError,
    "OverflowError": OverflowError,
}
_PUBLIC_ERROR_TYPES: dict[str, type[Exception]] = {
    "Protocol": ProtocolError,
    "ConnectionClosed": ConnectionClosedError,
    "Timeout": ResponseTimeoutError,
    "PoolBusy": PoolBusyError,
    "PushOverflow": PushOverflowError,
    "CloseTimeout": TransportCloseTimeoutError,
    "UnsupportedCommand": UnsupportedCommandError,
    "Internal": TransportError,
}


def _parts(error: BaseException) -> tuple[str, str, tuple[tuple[str, str], ...]]:
    """Read the tuple payload emitted by PyO3's private ``NativeError``."""

    raw: tuple[Any, ...] = error.args
    if len(raw) == 1 and isinstance(raw[0], tuple) and len(raw[0]) == 3:
        raw = raw[0]
    if len(raw) != 3:
        raise TypeError("malformed eltdx native error payload")
    kind, message, context = raw
    if not isinstance(kind, str) or not isinstance(message, str):
        raise TypeError("malformed eltdx native error kind or message")
    if not isinstance(context, Sequence) or isinstance(context, (str, bytes, bytearray)):
        raise TypeError("malformed eltdx native error context")
    normalized: list[tuple[str, str]] = []
    for item in context:
        if not isinstance(item, Sequence) or len(item) != 2:
            raise TypeError("malformed eltdx native error context entry")
        key, value = item
        if not isinstance(key, str) or not isinstance(value, str):
            raise TypeError("malformed eltdx native error context entry")
        normalized.append((key, value))
    return kind, message, tuple(normalized)


def _public_type(kind: str, context: tuple[tuple[str, str], ...]) -> type[Exception]:
    if kind == "InvalidArgument":
        python_kind = dict(context).get("python_kind", "ValueError")
        return _INVALID_ARGUMENT_TYPES.get(python_kind, ValueError)
    return _PUBLIC_ERROR_TYPES.get(kind, TransportError)


def map_native_error(error: BaseException) -> Exception:
    """Convert one private native error without changing its user message."""

    kind, message, context = _parts(error)
    mapped = _public_type(kind, context)(message)
    setattr(mapped, "_native_kind", kind)
    setattr(mapped, "_native_context", dict(context))
    return mapped


def call_native(function: Callable[..., T], /, *args: Any, **kwargs: Any) -> T:
    """Call native code and translate only the private structured exception."""

    try:
        return function(*args, **kwargs)
    except Exception as error:
        if error.__class__.__name__ != "NativeError":
            raise
        mapped = map_native_error(error)
        if error.__cause__ is None:
            raise mapped from None
        raise mapped from error.__cause__


def native_module() -> ModuleType:
    """Return the ABI-checked extension for transport facades."""

    return load_native()


__all__ = ["call_native", "map_native_error", "native_module"]
