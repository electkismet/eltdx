"""Import-free contracts for the E5 native protocol and actor facades."""

from __future__ import annotations

import ast
from pathlib import Path


ROOT = Path(__file__).parents[2]


def _source(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def _top_level_names(relative: str) -> set[str]:
    tree = ast.parse(_source(relative), filename=relative)
    names: set[str] = set()
    for node in tree.body:
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(node.name)
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    names.add(target.id)
    return names


def _top_level_import_names(relative: str) -> set[str]:
    tree = ast.parse(_source(relative), filename=relative)
    names: set[str] = set()
    for node in tree.body:
        if isinstance(node, ast.Import):
            for alias in node.names:
                names.update((alias.name, alias.name.rsplit(".", 1)[-1]))
        elif isinstance(node, ast.ImportFrom):
            if node.module is not None:
                names.update((node.module, node.module.rsplit(".", 1)[-1]))
            names.update(alias.name for alias in node.names)
    return names


def test_native_protocol_entrypoints_are_registered_and_stateless() -> None:
    source = _source("crates/eltdx-python/src/protocol.rs")
    request = _source("crates/eltdx-python/src/request.rs")
    lib = _source("crates/eltdx-python/src/lib.rs")
    for name in (
        "build_command_frame",
        "decode_response",
        "encode_request_frame",
        "parse_command_response",
    ):
        assert f"pub fn {name}" in source
        assert f'wrap_pyfunction!(protocol::{name}' in lib
    assert "NativeEngine" not in source
    assert source.count("request::from_python") == 2
    assert "PyResult<CommandRequest>" in request
    assert "CommandRequest::Heartbeat" in request
    assert "CommandResponse::parse" in source
    parse_body = source.split("pub fn parse_command_response", 1)[1]
    assert "decode_frame(response" not in parse_body
    assert "CommandResponse::parse(request, response)" in parse_body


def test_python_protocol_facades_have_no_python_wire_runtime() -> None:
    frame = _source("src/eltdx/protocol/frame.py")
    commands = _source("src/eltdx/protocol/commands/codec.py")
    assert "native_module()" in frame
    assert "native_module()" in commands
    assert "native_module().encode_request_frame" in frame
    assert "socket" not in frame.lower()
    assert "socket" not in commands.lower()
    assert "struct.pack" not in frame
    assert "build_command_frame" in commands
    assert "parse_command_response" in commands
    assert "response.data" in commands
    assert "response.raw" not in commands


def test_actor_module_is_snapshot_only() -> None:
    source = _source("src/eltdx/transport/actor.py")
    names = _top_level_names("src/eltdx/transport/actor.py")
    imports = _top_level_import_names("src/eltdx/transport/actor.py")
    assert names == {"RuntimeState", "TcpState", "ActorSnapshot", "__all__"}
    assert imports.isdisjoint({"threading", "socket", "ActorRuntime"})
    assert "ActorRuntime" not in source
    assert "def execute" not in source
    assert "def submit" not in source


def test_socket_and_pool_facades_have_no_python_runtime_core() -> None:
    socket_source = _source("src/eltdx/transport/socket.py")
    pool_source = _source("src/eltdx/transport/pool.py")
    for source in (socket_source, pool_source):
        assert "native_module().NativeEngine" in source
        assert "ActorRuntime" not in source
        assert "LeaseBroker" not in source
        assert "PushBuffer" not in source
        assert "threading" not in source
    assert "native_pin.execute" in pool_source
    assert "native_pin.close" in pool_source
    assert "session_snapshot" in socket_source
    assert "session_snapshot" in pool_source


def test_native_pin_and_diagnostics_paths_are_engine_owned() -> None:
    engine = _source("crates/eltdx-runtime/src/engine.rs")
    binding = _source("crates/eltdx-python/src/transport.rs")
    for command in ("OpenPin", "ExecutePinned", "ClosePin"):
        assert f"RuntimeCommand::{command}" in engine
    assert "terminal_pin(" in engine
    assert "terminal_pin_waiting(" in engine
    assert "PinHandle" in binding
    assert "NativePin" in binding
    assert "session_snapshot" in binding
    assert "pool_diagnostics" in binding
    assert "transport_diagnostics" in binding


def test_native_execute_and_push_dtos_reconstruct_public_values() -> None:
    socket_source = _source("src/eltdx/transport/socket.py")
    pool_source = _source("src/eltdx/transport/pool.py")
    assert "push_frame_from_dto" in socket_source
    assert "native_module().parse_command_response" in socket_source
    assert "response.data" in socket_source
    assert "response.raw" not in socket_source
    assert socket_source.count("response_from_dto(") >= 3
    assert "_push_value" in pool_source
    assert pool_source.count("response_from_dto(") >= 3


def test_native_execute_combines_admission_with_first_signal_poll() -> None:
    binding = _source("crates/eltdx-python/src/transport.rs")
    execute = binding.split("    fn execute(\n", 1)[1].split("\n    fn pin(", 1)[0]
    begin = "self.engine.begin_execute(request).map(|mut pending| {"
    first_poll = "let polled = pending.wait_timeout(SIGNAL_POLL_INTERVAL);"
    signal_check = "if let Err(signal_error) = py.check_signals() {"
    assert "let initial = py.detach(|| {" in execute
    assert begin in execute
    assert first_poll in execute
    assert execute.index(begin) < execute.index(first_poll) < execute.index(signal_check)
    assert "if let Ok((pending, _)) = initial" in execute
    assert "pending.cancel_and_confirm(CANCEL_CONFIRM_TIMEOUT)" in execute
    assert "let (mut pending, initial_poll) = initial.map_err(error::from_runtime)?;" in execute
    assert "PendingPoll::Ready(response) => response" in execute
    assert "let pending = py.detach(|| self.engine.begin_execute(request));" not in execute
