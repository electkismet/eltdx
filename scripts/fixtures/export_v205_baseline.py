"""Export authoritative v2.0.5 contracts and 7709 golden fixtures.

Run this script only in the isolated baseline-wheel environment created by the
unified test entrypoint. It intentionally imports ``eltdx`` lazily so invoking
``--help`` does not accidentally import the development tree.
"""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import importlib
import importlib.metadata
import inspect
import json
import os
import struct
import types
import typing
from dataclasses import MISSING, fields, is_dataclass
from datetime import date, datetime
from pathlib import Path
from typing import Any


BASELINE_TAG = "v2.0.5"
BASELINE_COMMIT = "6486a1692dd4aca5339001b2de22e88bb29e16ec"
BASELINE_VERSION = "2.0.5"
SCHEMA_VERSION = 1
MISSING_VALUE = object()

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FIXTURES_ROOT = REPOSITORY_ROOT / "tests" / "fixtures" / "7709"
DEFAULT_CONTRACT_OUTPUT = DEFAULT_FIXTURES_ROOT / "baseline" / BASELINE_TAG / "contracts.json"
MANIFEST_ROOT = REPOSITORY_ROOT / "tests" / "contracts" / "manifests"


def to_canonical(value: Any, *, wire_f32: bool = False) -> dict[str, Any]:
    """Encode a Python value without losing types, ordering, or float bits."""

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
    raise TypeError(f"unsupported canonical fixture value: {type(value).__module__}.{type(value).__qualname__}")


def from_canonical(value: dict[str, Any]) -> Any:
    """Decode request-side canonical values used by fixture inputs."""

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
        result = datetime.fromisoformat(value["value"])
        return result.replace(fold=value["fold"])
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


def annotation_shape(annotation: Any) -> dict[str, Any]:
    """Normalize evaluated annotations independently of repr formatting."""

    if annotation is None or annotation is type(None):
        return {"kind": "type", "path": "builtins:None"}
    if isinstance(annotation, str):
        return {"kind": "forward", "value": annotation}
    if isinstance(annotation, typing.ForwardRef):
        return {"kind": "forward", "value": annotation.__forward_arg__}
    if isinstance(annotation, typing.TypeVar):
        return {"kind": "typevar", "name": annotation.__name__}
    origin = typing.get_origin(annotation)
    arguments = typing.get_args(annotation)
    if origin in {typing.Union, types.UnionType}:
        return {"kind": "union", "args": [annotation_shape(item) for item in arguments]}
    if origin is typing.Literal:
        return {"kind": "literal", "values": [to_canonical(item) for item in arguments]}
    if origin is not None:
        return {
            "kind": "generic",
            "origin": annotation_shape(origin),
            "args": [annotation_shape(item) for item in arguments],
        }
    if isinstance(annotation, type):
        return {"kind": "type", "path": f"{annotation.__module__}:{annotation.__qualname__}"}
    return {"kind": "repr", "value": repr(annotation)}


def _annotation_or_missing(annotation: Any) -> dict[str, Any]:
    if annotation is inspect.Parameter.empty or annotation is inspect.Signature.empty:
        return {"kind": "missing"}
    return annotation_shape(annotation)


def _resolve(path: str) -> Any:
    module_name, qualname = path.split(":", 1)
    value: Any = importlib.import_module(module_name)
    for part in qualname.split("."):
        value = getattr(value, part)
    return value


def _signature_shape(value: Any) -> list[dict[str, Any]]:
    annotation_target = value.__init__ if inspect.isclass(value) else value
    try:
        annotations = inspect.get_annotations(annotation_target, eval_str=True)
    except (NameError, TypeError):
        annotations = inspect.get_annotations(annotation_target, eval_str=False)
    parameters = []
    for parameter in inspect.signature(value).parameters.values():
        item: dict[str, Any] = {
            "name": parameter.name,
            "kind": parameter.kind.name,
            "default": to_canonical(
                MISSING_VALUE if parameter.default is inspect.Parameter.empty else parameter.default
            ),
            "annotation": _annotation_or_missing(
                annotations.get(parameter.name, parameter.annotation)
            ),
        }
        parameters.append(item)
    signature = inspect.signature(value)
    return parameters + [
        {"return": _annotation_or_missing(annotations.get("return", signature.return_annotation))}
    ]


def _annotations_for(cls: type[Any]) -> dict[str, Any]:
    try:
        annotations = inspect.get_annotations(cls, eval_str=True)
    except (NameError, TypeError):
        annotations = inspect.get_annotations(cls, eval_str=False)
    return {name: annotation_shape(annotation) for name, annotation in annotations.items()}


def _load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def _public_contract_snapshot() -> dict[str, Any]:
    public_api = _load_json(MANIFEST_ROOT / "public_api.json")
    dataclasses_contract = _load_json(MANIFEST_ROOT / "dataclasses_exceptions.json")
    runtime_contract = _load_json(MANIFEST_ROOT / "runtime_surfaces.json")

    modules = {}
    for module_name in public_api["module_exports"]:
        try:
            modules[module_name] = list(importlib.import_module(module_name).__all__)
        except ImportError:
            # The current manifest may contain a module added after the
            # v2.0.5 baseline. Keep that absence explicit in the snapshot.
            modules[module_name] = {"missing": True}

    signatures: dict[str, Any] = {}
    for path, contract in public_api["classes"].items():
        try:
            cls = _resolve(path)
        except (AttributeError, ImportError):
            signatures[path] = {"missing": True}
            continue
        class_signatures = {}
        for name in contract["signatures"]:
            try:
                value = cls if name == "__call__" else getattr(cls, name)
            except AttributeError:
                # A method added after v2.0.5 is an intentional baseline
                # difference, not a reason to abort fixture generation.
                continue
            class_signatures[name] = _signature_shape(value)
        signatures[path] = class_signatures
    for path in public_api["callables"]:
        try:
            signatures[path] = _signature_shape(_resolve(path))
        except (AttributeError, ImportError):
            signatures[path] = {"missing": True}

    dataclass_snapshot: dict[str, Any] = {}
    for path in dataclasses_contract["dataclasses"]:
        try:
            cls = _resolve(path)
        except (AttributeError, ImportError):
            dataclass_snapshot[path] = {"missing": True}
            continue
        dataclass_snapshot[path] = {
            "annotations": _annotations_for(cls),
            "fields": [field.name for field in fields(cls)],
        }

    exceptions = {}
    for path in dataclasses_contract["exceptions"]:
        try:
            cls = _resolve(path)
        except (AttributeError, ImportError):
            exceptions[path] = {"missing": True}
            continue
        exceptions[path] = [f"{base.__module__}:{base.__qualname__}" for base in cls.__bases__]
    return {
        "module_exports": modules,
        "signatures": signatures,
        "dataclasses": dataclass_snapshot,
        "exceptions": exceptions,
        "protocol_exports": runtime_contract["protocol"]["module_exports"],
    }


class _ParserCaptured(Exception):
    def __init__(self, parser: argparse.ArgumentParser) -> None:
        self.parser = parser


def _capture_parser(main: Any) -> argparse.ArgumentParser:
    original = argparse.ArgumentParser.parse_args

    def capture(parser: argparse.ArgumentParser, *_args: Any, **_kwargs: Any) -> Any:
        raise _ParserCaptured(parser)

    argparse.ArgumentParser.parse_args = capture
    try:
        main()
    except _ParserCaptured as captured:
        return captured.parser
    finally:
        argparse.ArgumentParser.parse_args = original
    raise RuntimeError("CLI main returned without parsing arguments")


def _cli_snapshot() -> dict[str, Any]:
    result: dict[str, Any] = {}
    for command, path in {
        "eltdx-smoke": "eltdx.smoke:main",
        "eltdx-f10-smoke": "eltdx.f10_smoke:main",
    }.items():
        parser = _capture_parser(_resolve(path))
        actions = []
        for action in parser._actions:
            actions.append(
                {
                    "option_strings": list(action.option_strings),
                    "dest": action.dest,
                    "required": action.required,
                    "default": to_canonical(action.default),
                    "type": getattr(action.type, "__name__", None),
                    "action": type(action).__name__,
                    "choices": to_canonical(tuple(action.choices)) if action.choices is not None else None,
                }
            )
        result[command] = {
            "description": parser.description,
            "actions": actions,
        }
    return result


def _model_dump(value: Any) -> Any:
    if hasattr(value, "model_dump"):
        return value.model_dump(mode="json", exclude_none=False)
    if hasattr(value, "dict"):
        return value.dict()
    raise TypeError(f"cannot export MCP model {type(value)!r}")


async def _mcp_snapshot_async() -> dict[str, Any]:
    try:
        from mcp import Client
    except ImportError as exc:
        raise RuntimeError("baseline export requires the v2.0.5 mcp extra") from exc

    create_mcp_server = _resolve("eltdx.mcp:create_mcp_server")
    async with Client(create_mcp_server()) as client:
        tools = await client.list_tools()
        resources = await client.list_resources()
    return {
        "tools": [_model_dump(tool) for tool in tools.tools],
        "resources": [_model_dump(resource) for resource in resources.resources],
    }


def _mcp_snapshot() -> dict[str, Any]:
    return asyncio.run(_mcp_snapshot_async())


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def assert_baseline_distribution(wheel: Path) -> dict[str, str]:
    if not wheel.is_file():
        raise FileNotFoundError(f"baseline wheel does not exist: {wheel}")
    installed_version = importlib.metadata.version("eltdx")
    module = importlib.import_module("eltdx")
    if installed_version != BASELINE_VERSION or module.__version__ != BASELINE_VERSION:
        raise RuntimeError(
            f"baseline exporter requires installed eltdx=={BASELINE_VERSION}; "
            f"distribution={installed_version!r}, module={module.__version__!r}"
        )
    module_path = Path(module.__file__).resolve()
    distribution = importlib.metadata.distribution("eltdx")
    distribution_root = Path(distribution.locate_file("")).resolve()
    source_root = (REPOSITORY_ROOT / "src").resolve()
    if module_path.is_relative_to(source_root):
        raise RuntimeError(f"baseline exporter imported the development source tree: {module_path}")
    return {
        "tag": BASELINE_TAG,
        "commit": BASELINE_COMMIT,
        "version": BASELINE_VERSION,
        "wheel": wheel.name,
        "wheel_sha256": _sha256(wheel),
        "module_file": module_path.relative_to(distribution_root).as_posix(),
    }


def _frame_header(frame: Any) -> dict[str, Any]:
    length = len(frame.data) + 2
    return {
        "control": frame.control,
        "message_id": frame.msg_id,
        "message_type": frame.msg_type,
        "zip_length": length,
        "length": length,
    }


def _fixture_cases(fixtures_root: Path) -> list[Path]:
    return sorted(
        metadata.parent
        for metadata in fixtures_root.glob("*/*/metadata.json")
        if metadata.parent.parent.name not in {"baseline"}
    )


def _write_bytes(path: Path, value: bytes, *, force: bool) -> None:
    if path.exists() and not force:
        raise FileExistsError(f"refusing to overwrite generated fixture: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_bytes(value)
    temporary.replace(path)


def _write_json(path: Path, value: Any, *, force: bool) -> None:
    if path.exists() and not force:
        raise FileExistsError(f"refusing to overwrite generated fixture: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=False) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


def export_fixture_case(case: Path, provenance: dict[str, str], *, force: bool) -> None:
    from eltdx.protocol import build_command_frame, decode_response, parse_command_response

    request_path = case / "request.json"
    response_path = case / "response.bin"
    metadata_path = case / "metadata.json"
    for required in (request_path, response_path, metadata_path):
        if not required.is_file():
            raise FileNotFoundError(f"incomplete fixture case {case}: missing {required.name}")

    metadata = _load_json(metadata_path)
    command_code = int(metadata["command_code"])
    message_id = int(metadata["message_id"])
    if not 1 <= message_id <= 0xFFFFFFFF:
        raise ValueError(f"fixture message_id must be fixed and nonzero: {case}")
    request_payload = from_canonical(_load_json(request_path))
    if not isinstance(request_payload, dict):
        raise TypeError(f"fixture request must decode to dict: {case}")
    request_context = to_canonical(request_payload)

    expected_exception = None
    try:
        request_frame = build_command_frame(command_code, request_payload, message_id)
        request_bytes = request_frame.to_bytes()
        metadata["frame_header"] = to_canonical(_frame_header(request_frame))
    except Exception as error:
        expected_exception = canonical_exception(error, phase="build")
        request_bytes = b""

    expected = None
    if expected_exception is None:
        try:
            response = decode_response(response_path.read_bytes())
            if response.msg_id != message_id:
                raise ValueError(
                    f"response message id {response.msg_id} does not match fixed fixture id {message_id}"
                )
            if response.msg_type != command_code:
                raise ValueError(
                    f"response message type {response.msg_type} does not match command {command_code}"
                )
            parsed = parse_command_response(command_code, response, request_payload)
            expected = to_canonical(parsed)
        except Exception as error:
            expected_exception = canonical_exception(error, phase="parse")

    metadata.update(
        {
            "schema_version": SCHEMA_VERSION,
            "baseline_tag": BASELINE_TAG,
            "baseline_commit": BASELINE_COMMIT,
            "baseline_wheel_sha256": provenance["wheel_sha256"],
            "request_context": request_context,
            "expected_exception": expected_exception,
        }
    )
    _write_bytes(case / "request.bin", request_bytes, force=force)
    _write_json(case / "expected.json", expected or {"$type": "missing"}, force=force)
    _write_json(metadata_path, metadata, force=True)


def export_all(
    *,
    wheel: Path,
    fixtures_root: Path,
    contract_output: Path,
    force: bool,
) -> dict[str, Any]:
    provenance = assert_baseline_distribution(wheel)
    cases = _fixture_cases(fixtures_root)
    if not cases:
        raise RuntimeError(f"no fixture cases found below {fixtures_root}")
    if not force:
        conflicts = [contract_output] if contract_output.exists() else []
        conflicts.extend(
            output
            for case in cases
            for output in (case / "request.bin", case / "expected.json")
            if output.exists()
        )
        if conflicts:
            rendered = "\n".join(f"- {path}" for path in conflicts)
            raise FileExistsError(f"refusing partial baseline export; generated outputs exist:\n{rendered}")

    snapshot = {
        "schema_version": SCHEMA_VERSION,
        "baseline": provenance,
        "public": _public_contract_snapshot(),
        "cli": _cli_snapshot(),
        "mcp": _mcp_snapshot(),
    }
    _write_json(contract_output, snapshot, force=force)
    for case in cases:
        export_fixture_case(case, provenance, force=force)
    return {"contracts": str(contract_output), "fixture_cases": len(cases), "baseline": provenance}


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Export frozen eltdx v2.0.5 baseline expectations")
    parser.add_argument("--wheel", type=Path, required=True, help="exact installed v2.0.5 wheel file")
    parser.add_argument("--fixtures-root", type=Path, default=DEFAULT_FIXTURES_ROOT)
    parser.add_argument("--contract-output", type=Path, default=DEFAULT_CONTRACT_OUTPUT)
    parser.add_argument("--force", action="store_true", help="replace previously generated baseline outputs")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    result = export_all(
        wheel=args.wheel.resolve(),
        fixtures_root=args.fixtures_root.resolve(),
        contract_output=args.contract_output.resolve(),
        force=args.force,
    )
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
