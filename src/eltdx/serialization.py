"""JSON serialization helpers for eltdx return models."""

from __future__ import annotations

import json
from dataclasses import fields, is_dataclass
from datetime import date, datetime
from pathlib import Path
from typing import Any

from .models.trade_batch import TradeBatch


def to_jsonable(value: Any) -> Any:
    """Convert eltdx models and common Python objects to JSON-safe values."""

    if value is None or isinstance(value, (str, int, float, bool)):
        return value

    if isinstance(value, (datetime, date)):
        return value.isoformat()

    if isinstance(value, bytes):
        return value.hex()

    if isinstance(value, TradeBatch):
        # Explicit row-oriented export pays the full model construction cost.
        return to_jsonable(value.to_page())

    if is_dataclass(value) and not isinstance(value, type):
        return {field.name: to_jsonable(getattr(value, field.name)) for field in fields(value)}

    if isinstance(value, dict):
        return {str(to_jsonable(key)): to_jsonable(item) for key, item in value.items()}

    if isinstance(value, (list, tuple, set, frozenset)):
        return [to_jsonable(item) for item in value]

    if isinstance(value, Path):
        return str(value)

    raise TypeError(f"object of type {type(value).__name__} is not JSON serializable")


def to_json(value: Any, *, ensure_ascii: bool = False, indent: int | None = None) -> str:
    """Serialize eltdx models to a JSON string."""

    return json.dumps(to_jsonable(value), ensure_ascii=ensure_ascii, indent=indent)
