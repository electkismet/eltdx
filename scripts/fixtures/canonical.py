"""Lossless canonical values used by protocol golden fixtures."""

from __future__ import annotations

import struct
from dataclasses import MISSING, fields, is_dataclass
from datetime import date, datetime
from typing import Any


MISSING_VALUE = object()


def to_canonical(value: Any, *, wire_f32: bool = False) -> dict[str, Any]:
    if value is MISSING_VALUE or value is MISSING:
        return {"$type": "missing"}
    if value is None:
        return {"$type": "none"}
    if isinstance(value, bool):
        return {"$type": "bool", "value": value}
    if isinstance(value, int):
        return {"$type": "int", "value": str(value)}
    if isinstance(value, float):
        result = {
            "$type": "float",
            "f64_bits": struct.pack(">d", value).hex(),
            "readable_hex": value.hex(),
        }
        if wire_f32:
            result["wire_f32_bits"] = struct.pack(">f", value).hex()
        return result
    if isinstance(value, str):
        return {"$type": "str", "value": value}
    if isinstance(value, bytes):
        return {"$type": "bytes", "hex": value.hex()}
    if isinstance(value, datetime):
        timezone = None
        if value.tzinfo is not None:
            timezone = getattr(value.tzinfo, "key", None) or value.tzname() or str(value.utcoffset())
        return {
            "$type": "datetime",
            "value": value.isoformat(),
            "timezone": timezone,
            "fold": value.fold,
        }
    if isinstance(value, date):
        return {"$type": "date", "value": value.isoformat()}
    if isinstance(value, tuple):
        return {"$type": "tuple", "items": [to_canonical(item) for item in value]}
    if isinstance(value, list):
        return {"$type": "list", "items": [to_canonical(item) for item in value]}
    if isinstance(value, dict):
        return {
            "$type": "dict",
            "items": [[to_canonical(key), to_canonical(item)] for key, item in value.items()],
        }
    if is_dataclass(value) and not isinstance(value, type):
        return {
            "$type": "dataclass",
            "module": type(value).__module__,
            "qualname": type(value).__qualname__,
            "fields": [
                [
                    field.name,
                    to_canonical(
                        getattr(value, field.name),
                        wire_f32=field.name.endswith(("_raw_f32", "_raw_float")),
                    ),
                ]
                for field in fields(value)
            ],
        }
    raise TypeError(
        f"unsupported canonical fixture value: {type(value).__module__}.{type(value).__qualname__}"
    )


def from_canonical(value: dict[str, Any]) -> Any:
    kind = value["$type"]
    if kind == "missing":
        return MISSING_VALUE
    if kind == "none":
        return None
    if kind in {"bool", "str"}:
        return value["value"]
    if kind == "int":
        return int(value["value"])
    if kind == "float":
        return struct.unpack(">d", bytes.fromhex(value["f64_bits"]))[0]
    if kind == "bytes":
        return bytes.fromhex(value["hex"])
    if kind == "date":
        return date.fromisoformat(value["value"])
    if kind == "datetime":
        return datetime.fromisoformat(value["value"]).replace(fold=value["fold"])
    if kind == "tuple":
        return tuple(from_canonical(item) for item in value["items"])
    if kind == "list":
        return [from_canonical(item) for item in value["items"]]
    if kind == "dict":
        return {from_canonical(key): from_canonical(item) for key, item in value["items"]}
    raise TypeError(f"canonical request value cannot be reconstructed: {kind}")


def canonical_exception(error: BaseException, *, phase: str) -> dict[str, Any]:
    context = getattr(error, "context", MISSING_VALUE)
    cause = error.__cause__
    return {
        "phase": phase,
        "type": f"{type(error).__module__}:{type(error).__qualname__}",
        "message": str(error),
        "context": to_canonical(context),
        "cause": canonical_exception(cause, phase="cause") if cause is not None else None,
    }


def frame_header(frame: Any) -> dict[str, Any]:
    length = len(frame.data) + 2
    return {
        "control": frame.control,
        "message_id": frame.msg_id,
        "message_type": frame.msg_type,
        "zip_length": length,
        "length": length,
    }
