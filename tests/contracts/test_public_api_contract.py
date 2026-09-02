"""Public import, namespace, and signature contracts."""

from __future__ import annotations

import importlib
import inspect
import json
from pathlib import Path
from typing import Any


MANIFEST = Path(__file__).with_name("manifests") / "public_api.json"


def _load_manifest() -> dict[str, Any]:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def _resolve(path: str) -> Any:
    module_name, qualname = path.split(":", 1)
    value: Any = importlib.import_module(module_name)
    for part in qualname.split("."):
        value = getattr(value, part)
    return value


def _signature_shape(value: Any) -> str:
    parameters = list(inspect.signature(value).parameters.values())
    parts: list[str] = []
    inserted_keyword_separator = False
    for index, parameter in enumerate(parameters):
        if parameter.kind is inspect.Parameter.KEYWORD_ONLY and not inserted_keyword_separator:
            if not any(item.kind is inspect.Parameter.VAR_POSITIONAL for item in parameters[:index]):
                parts.append("*")
            inserted_keyword_separator = True

        if parameter.kind is inspect.Parameter.VAR_POSITIONAL:
            text = f"*{parameter.name}"
            inserted_keyword_separator = True
        elif parameter.kind is inspect.Parameter.VAR_KEYWORD:
            text = f"**{parameter.name}"
        else:
            text = parameter.name

        if parameter.default is not inspect.Parameter.empty:
            text += f"={parameter.default!r}"
        parts.append(text)

        if parameter.kind is inspect.Parameter.POSITIONAL_ONLY:
            next_kind = parameters[index + 1].kind if index + 1 < len(parameters) else None
            if next_kind is not inspect.Parameter.POSITIONAL_ONLY:
                parts.append("/")
    return f"({', '.join(parts)})"


def _public_members(cls: type[Any]) -> list[str]:
    result = []
    for name, value in vars(cls).items():
        if name.startswith("_"):
            continue
        if inspect.isfunction(value) or isinstance(value, (classmethod, staticmethod, property)):
            result.append(name)
    return sorted(result)


def test_module_exports_retain_legacy_members() -> None:
    manifest = _load_manifest()
    for module_name, expected in manifest["module_exports"].items():
        module = importlib.import_module(module_name)
        assert set(expected) <= set(module.__all__)


def test_public_class_members_and_signatures_match_v205() -> None:
    manifest = _load_manifest()
    for path, contract in manifest["classes"].items():
        cls = _resolve(path)
        assert _public_members(cls) == contract["members"]
        for name, expected in contract["signatures"].items():
            value = cls if name == "__call__" else getattr(cls, name)
            assert _signature_shape(value) == expected, f"signature drift for {path}.{name}"


def test_public_function_signatures_match_v205() -> None:
    manifest = _load_manifest()
    for path, expected in manifest["callables"].items():
        assert _signature_shape(_resolve(path)) == expected, f"signature drift for {path}"


def test_aliases_and_client_namespaces_match_v205() -> None:
    manifest = _load_manifest()
    for left, right in manifest["aliases"]:
        assert _resolve(left) is _resolve(right)

    client = _resolve("eltdx:TdxClient").in_memory()
    for name, expected_type in manifest["client_namespaces"].items():
        assert type(getattr(client, name)) is _resolve(expected_type)


def test_removed_flat_client_api_stays_removed() -> None:
    manifest = _load_manifest()
    client = _resolve("eltdx:TdxClient").in_memory()
    for name in manifest["removed_client_attributes"]:
        assert not hasattr(client, name)
    assert all(not name.startswith("get_") for name in vars(type(client.helpers)))
