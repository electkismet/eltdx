"""Generate or verify version-independent 7709 protocol golden fixtures."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.fixtures.canonical import (  # noqa: E402
    canonical_exception,
    frame_header,
    from_canonical,
    to_canonical,
)


DEFAULT_FIXTURES_ROOT = ROOT / "tests" / "fixtures" / "7709"


def _json_bytes(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def _outputs(case: Path) -> dict[str, bytes]:
    from eltdx.protocol import (
        build_command_frame,
        decode_response,
        parse_command_response,
    )

    metadata = json.loads((case / "metadata.json").read_text(encoding="utf-8"))
    request = from_canonical(
        json.loads((case / "request.json").read_text(encoding="utf-8"))
    )
    command = int(metadata["command_code"])
    message_id = int(metadata["message_id"])
    expected_exception = None
    try:
        frame = build_command_frame(command, request, message_id)
        request_bytes = frame.to_bytes()
        metadata["frame_header"] = to_canonical(frame_header(frame))
    except Exception as error:
        expected_exception = canonical_exception(error, phase="build")
        request_bytes = b""

    expected: dict[str, Any] = {"$type": "missing"}
    if expected_exception is None:
        try:
            response = decode_response((case / "response.bin").read_bytes())
            if response.msg_id != message_id:
                raise ValueError(
                    f"response message id {response.msg_id} does not match fixed fixture id {message_id}"
                )
            if response.msg_type != command:
                raise ValueError(
                    f"response message type {response.msg_type} does not match command {command}"
                )
            expected = to_canonical(parse_command_response(command, response, request))
        except Exception as error:
            expected_exception = canonical_exception(error, phase="parse")

    metadata["golden_schema_version"] = 1
    metadata["request_context"] = to_canonical(request)
    metadata["expected_exception"] = expected_exception
    return {
        "request.bin": request_bytes,
        "expected.json": _json_bytes(expected),
        "metadata.json": _json_bytes(metadata),
    }


def process(fixtures_root: Path, *, check: bool) -> list[str]:
    differences: list[str] = []
    for metadata_path in sorted(fixtures_root.glob("*/*/metadata.json")):
        case = metadata_path.parent
        for name, content in _outputs(case).items():
            path = case / name
            if check:
                if not path.is_file() or path.read_bytes() != content:
                    differences.append(path.relative_to(fixtures_root).as_posix())
            else:
                path.write_bytes(content)
    return differences


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixtures-root", type=Path, default=DEFAULT_FIXTURES_ROOT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    differences = process(args.fixtures_root.resolve(), check=args.check)
    for path in differences:
        print(path)
    return 1 if differences else 0


if __name__ == "__main__":
    raise SystemExit(main())
