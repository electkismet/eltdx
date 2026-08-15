"""Protocol, transport, MCP, CLI, and removed-API contracts for 3.0."""

from __future__ import annotations

import argparse
import importlib
import importlib.util
import inspect
import json
import sys
from dataclasses import fields, is_dataclass
from pathlib import Path
from typing import Any

import pytest


MANIFEST = Path(__file__).with_name("manifests") / "runtime_surfaces.json"


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


def _public_properties(cls: type[Any]) -> list[str]:
    return sorted(name for name, value in vars(cls).items() if isinstance(value, property) and not name.startswith("_"))


def test_protocol_facade_exports_and_signatures() -> None:
    contract = _load_manifest()["protocol"]
    for module_name, expected in contract["module_exports"].items():
        assert importlib.import_module(module_name).__all__ == expected
    for path, expected in contract["callables"].items():
        assert _signature_shape(_resolve(path)) == expected, path


def test_protocol_wire_facade_is_native_only() -> None:
    contract = _load_manifest()["protocol"]
    for path in contract["native_wire_entrypoints"]:
        value = _resolve(path)
        source = inspect.getsource(value)
        assert "_native" in source, f"{path} does not delegate to eltdx._native"
    for module_name in contract["forbidden_runtime_modules"]:
        assert importlib.util.find_spec(module_name) is None, module_name


def test_diagnostics_enums_and_dataclasses_match_contract() -> None:
    contract = _load_manifest()["diagnostics"]
    for path, expected in contract["enums"].items():
        assert list(_resolve(path).__members__) == expected, path
    for path, expected in contract["dataclasses"].items():
        cls = _resolve(path)
        assert is_dataclass(cls), path
        assert [field.name for field in fields(cls)] == expected, path
        assert tuple(cls.__slots__) == tuple(expected), path
        assert cls.__dataclass_params__.frozen is True, path


def test_transport_public_signatures_and_properties() -> None:
    for path, contract in _load_manifest()["transports"].items():
        cls = _resolve(path)
        if "constructor" in contract:
            assert _signature_shape(cls) == contract["constructor"], path
        for name, expected in contract["methods"].items():
            assert _signature_shape(getattr(cls, name)) == expected, f"{path}.{name}"
        assert _public_properties(cls) == contract["properties"], path


def test_pinned_proxy_contract_is_owned_by_pool_context_manager() -> None:
    contract = _load_manifest()["transports"]["eltdx.transport.pool:PinnedTransportProxy"]
    owner = _resolve(contract["context_manager_owner"])
    assert hasattr(owner, "__wrapped__"), "pin() must remain a contextmanager"
    assert contract["close_shared_pool"] is False
    assert contract["old_epoch_invalid"] is True


def test_mcp_tool_and_resource_snapshot() -> None:
    from eltdx import mcp as mcp_module

    contract = _load_manifest()["mcp"]
    assert mcp_module._MCP_POOL_SIZE == contract["pool_size"]
    assert mcp_module._MAX_CLIENTS == contract["max_clients"]

    registry = mcp_module._ClientRegistry()
    tools = mcp_module._McpTools(registry)
    try:
        for name, tool_contract in contract["tools"].items():
            method = getattr(tools, tool_contract["method"])
            assert _signature_shape(method) == tool_contract["signature"], name
            return_annotation = inspect.signature(method).return_annotation
            expected_origin = "list" if tool_contract["output"] == "array" else "dict"
            assert str(return_annotation).startswith(expected_origin), name
        assert set(mcp_module._DOC_PATHS) == {
            uri.rsplit("/", 1)[-1] for uri in contract["resources"]
        }
        for uri, resource in contract["resources"].items():
            key = uri.rsplit("/", 1)[-1]
            assert mcp_module._DOC_PATHS[key] == resource["path"]
    finally:
        registry.close()


def test_mcp_sdk_generated_output_schemas_and_resources() -> None:
    try:
        from mcp import Client
    except ImportError:
        pytest.skip("MCP SDK 2 optional dependency is not installed")

    import asyncio

    from eltdx.mcp import create_mcp_server

    contract = _load_manifest()["mcp"]

    async def exercise() -> None:
        async with Client(create_mcp_server()) as client:
            tools = await client.list_tools()
            assert {tool.name for tool in tools.tools} == set(contract["tools"])
            for tool in tools.tools:
                method_name = contract["tools"][tool.name]["method"]
                signature = inspect.signature(getattr(_resolve("eltdx.mcp:_McpTools"), method_name))
                parameters = [
                    parameter
                    for parameter in signature.parameters.values()
                    if parameter.name != "self"
                ]
                assert set(tool.input_schema["properties"]) == {
                    parameter.name for parameter in parameters
                }
                assert set(tool.input_schema.get("required", [])) == {
                    parameter.name
                    for parameter in parameters
                    if parameter.default is inspect.Parameter.empty
                }
                assert tool.output_schema is not None, tool.name
                expected = contract["tools"][tool.name]["output"]
                if expected == "array":
                    assert tool.output_schema["properties"]["result"]["type"] == "array"
                else:
                    assert tool.output_schema["type"] == "object"

            resources = await client.list_resources()
            observed = {
                str(resource.uri): {
                    "name": resource.name,
                    "title": resource.title,
                    "mime_type": resource.mime_type,
                }
                for resource in resources.resources
            }
            expected = {
                uri: {key: value for key, value in resource.items() if key != "path"}
                for uri, resource in contract["resources"].items()
            }
            assert observed == expected

    asyncio.run(exercise())


class _ParserCaptured(Exception):
    def __init__(self, parser: argparse.ArgumentParser) -> None:
        self.parser = parser


def _capture_parser(monkeypatch: pytest.MonkeyPatch, main: Any) -> argparse.ArgumentParser:
    def capture(parser: argparse.ArgumentParser) -> Any:
        raise _ParserCaptured(parser)

    monkeypatch.setattr(argparse.ArgumentParser, "parse_args", capture)
    with pytest.raises(_ParserCaptured) as captured:
        main()
    return captured.value.parser


def _parser_options(parser: argparse.ArgumentParser) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for action in parser._actions:
        if action.dest == "help":
            continue
        option = action.option_strings[-1]
        if isinstance(action, argparse._StoreTrueAction):
            action_name = "store_true"
        else:
            action_name = "store"
        item: dict[str, Any] = {"default": action.default, "action": action_name}
        if action.type is not None:
            item["type"] = action.type.__name__
        result[option] = item
    return result


@pytest.mark.parametrize("command", ["eltdx-smoke", "eltdx-f10-smoke"])
def test_cli_parser_defaults(monkeypatch: pytest.MonkeyPatch, command: str) -> None:
    contract = _load_manifest()["cli"][command]
    parser = _capture_parser(monkeypatch, _resolve(contract["main"]))
    assert _parser_options(parser) == contract["options"]


@pytest.mark.parametrize("command", ["eltdx-smoke", "eltdx-f10-smoke"])
def test_cli_help_and_parse_error_exit_codes(monkeypatch: pytest.MonkeyPatch, command: str) -> None:
    contract = _load_manifest()["cli"][command]
    main = _resolve(contract["main"])
    monkeypatch.setattr(sys, "argv", [command, "--help"])
    with pytest.raises(SystemExit) as help_exit:
        main()
    assert help_exit.value.code == contract["exit_codes"]["help"]

    monkeypatch.setattr(sys, "argv", [command, "--not-an-option"])
    with pytest.raises(SystemExit) as parse_exit:
        main()
    assert parse_exit.value.code == contract["exit_codes"]["parse_error"]


def test_smoke_invalid_counts_exit_nonzero(monkeypatch: pytest.MonkeyPatch) -> None:
    contract = _load_manifest()["cli"]["eltdx-smoke"]
    main = _resolve(contract["main"])
    monkeypatch.setattr(sys, "argv", ["eltdx-smoke", "--quote-count", "0"])
    with pytest.raises(SystemExit) as invalid:
        main()
    assert invalid.value.code
    assert contract["exit_codes"]["invalid_counts"] == 1


def test_installed_console_entry_points_match_contract() -> None:
    from importlib.metadata import distribution

    expected = _load_manifest()["cli"]["entry_points"]
    observed = {
        entry.name: entry.value
        for entry in distribution("eltdx").entry_points
        if entry.group == "console_scripts" and entry.name in expected
    }
    assert observed == expected


def test_console_entry_points_preserve_success_codes(monkeypatch: pytest.MonkeyPatch) -> None:
    from eltdx import cli, mcp

    monkeypatch.setattr(cli, "smoke_main", lambda: 0)
    monkeypatch.setattr(cli, "f10_smoke_main", lambda: 0)
    assert cli.smoke() == 0
    assert cli.f10_smoke() == 0

    calls: list[str] = []

    class Server:
        def run(self, transport: str) -> None:
            calls.append(transport)

    monkeypatch.setattr(mcp, "create_mcp_server", Server)
    assert mcp.main() == 0
    assert calls == [_load_manifest()["cli"]["eltdx-mcp"]["transport"]]


def test_removed_flat_client_api_stays_removed() -> None:
    from eltdx import TdxClient

    client = TdxClient.in_memory()
    for name in _load_manifest()["removed_client_attributes"]:
        assert not hasattr(client, name), name
    assert all(not name.startswith("get_") for name in vars(type(client.helpers)))
