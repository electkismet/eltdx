"""Thin command codec facade delegating all 7709 wire work to Rust."""

from __future__ import annotations

from typing import Any

from eltdx._native_models import response_from_dto
from eltdx.protocol.frame import RequestFrame, ResponseFrame
from eltdx.transport.native import call_native, native_module


def build_command_frame(command: int, payload: dict[str, Any] | None, msg_id: int) -> RequestFrame:
    dto = call_native(native_module().build_command_frame, command, payload or {}, msg_id)
    if not isinstance(dto, tuple) or len(dto) != 4:
        raise TypeError("native request frame DTO must contain four fields")
    return RequestFrame(*dto)


def parse_command_response(
    command: int,
    response: ResponseFrame,
    request_payload: dict[str, Any] | None = None,
) -> Any:
    if not isinstance(response, ResponseFrame):
        raise TypeError("response must be a ResponseFrame")
    dto = call_native(
        native_module().parse_command_response,
        command,
        response.raw,
        request_payload or {},
    )
    return response_from_dto(dto)


__all__ = ["build_command_frame", "parse_command_response"]
