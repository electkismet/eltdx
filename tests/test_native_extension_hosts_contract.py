"""EX host configuration object contracts; execute after the complete source gate."""
from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from eltdx.ex import hosts


@pytest.mark.parametrize("value,expected", [
    (" example.com ", "example.com"), (" [::1] ", "[::1]"),
    ("[opaque]", "[opaque]"), ("[]", "[]"), ("[" + "x" * 300 + "]", "[" + "x" * 300 + "]"),
    ("host:7727", None), ("https://host", None), ("x" * 254, None),
    ("a\u2003b", None), ("a\x1cb", None), (1, None), (None, None),
])
def test_native_hosts_normalization_keeps_bare_host_and_bracket_rules(value, expected):
    assert hosts.normalize_ex_host(value) == expected


def test_native_hosts_normalization_returns_original_strip_object(monkeypatch):
    class Text(str):
        def strip(self):
            return self

    value = Text("host.example")
    assert hosts.normalize_ex_host(value) is value


def test_native_hosts_space_generator_is_lazy_and_captures_any(monkeypatch):
    events = []

    class Character:
        def isspace(self):
            events.append("space")
            return True

    class Text(str):
        def strip(self):
            return self

        def __iter__(self):
            events.append("iter")
            monkeypatch.setattr(hosts, "any", lambda values: pytest.fail("any captured before iter"), raising=False)
            return iter((Character(),))

    def first(values):
        events.append("any")
        return next(values)

    monkeypatch.setattr(hosts, "any", first, raising=False)
    assert hosts.normalize_ex_host(Text("x")) is None
    assert events == ["iter", "any", "space"]


def test_native_hosts_space_generator_converts_body_stopiteration(monkeypatch):
    original = StopIteration("character")

    class Character:
        def isspace(self):
            raise original

    class Text(str):
        def strip(self):
            return self

        def __iter__(self):
            return iter((Character(),))

    with pytest.raises(RuntimeError, match="generator raised StopIteration") as caught:
        hosts.normalize_ex_host(Text("x"))
    assert caught.value.__cause__ is original


def test_native_hosts_parse_uses_live_normalizer_and_equality_not_hashing(monkeypatch):
    first, second = ["one"], ["two"]
    responses = iter((first, second, ["one"]))
    monkeypatch.setattr(hosts, "normalize_ex_host", lambda value: next(responses))
    result = hosts.parse_ex_hosts([1, 2, 3], source="x")
    assert result == [first, second] and result[0] is first and result[1] is second


def test_native_hosts_error_captures_constructor_before_format_and_repr(monkeypatch):
    events = []
    error = ValueError("captured")

    class Source:
        def __format__(self, spec):
            events.append("source")
            monkeypatch.setattr(hosts, "ValueError", lambda text: pytest.fail("error already captured"), raising=False)
            return "origin"

    class Value:
        def __repr__(self):
            events.append("repr")
            monkeypatch.setattr(hosts, "DEFAULT_EX_PORT", 9000)
            return "invalid"

    monkeypatch.setattr(hosts, "ValueError", lambda text: events.append(text) or error, raising=False)
    with pytest.raises(ValueError) as caught:
        hosts.parse_ex_hosts([Value()], source=Source())
    assert caught.value is error
    assert events == ["source", "repr", "origin 中的主机项无效: invalid；只接受裸主机名/IP（例如 '203.0.113.10'），端口由 ExTdxClient(port=) 指定，默认 9000"]


def test_native_hosts_env_repeats_strip_and_reads_source_after_split(monkeypatch):
    events = []

    class Text:
        def strip(self):
            events.append("strip")
            return "raw"

    class Split:
        def split(self, value):
            events.append(("split", value))
            monkeypatch.setattr(hosts, "EX_HOSTS_ENV", "changed")
            return ["a", "", "b"]

    monkeypatch.setattr(hosts, "_HOST_SPLIT", Split())
    monkeypatch.setattr(hosts, "parse_ex_hosts", lambda values, **kwargs: events.append((values, kwargs)) or values)
    assert hosts.load_ex_hosts_from_env({hosts.EX_HOSTS_ENV: Text()}) == ["a", "b"]
    assert events == ["strip", "strip", ("split", "raw"), (["a", "b"], {"source": "changed"})]


def test_native_hosts_priority_is_lazy_and_returns_selected_object(monkeypatch):
    chosen = ["selected"]
    monkeypatch.setattr(hosts, "load_ex_hosts_from_env", lambda env: chosen)
    monkeypatch.setattr(hosts, "load_ex_hosts_from_file", lambda path: pytest.fail("file is lower priority"))
    assert hosts.load_ex_hosts(env={}) is chosen
    monkeypatch.setattr(hosts, "load_ex_hosts_from_env", lambda env: [])
    monkeypatch.setattr(hosts, "load_ex_hosts_from_file", lambda path: chosen)
    assert hosts.load_ex_hosts(env={}) is chosen
    monkeypatch.setattr(hosts, "load_ex_hosts_from_file", lambda path: [])
    defaults = ["fallback"]
    monkeypatch.setattr(hosts, "DEFAULT_EX_HOSTS", defaults)
    result = hosts.load_ex_hosts(env={})
    assert result == defaults and result is not defaults


def test_native_hosts_invalid_higher_priority_source_does_not_fall_back(monkeypatch):
    def fail(env):
        raise ValueError("bad env")

    monkeypatch.setattr(hosts, "load_ex_hosts_from_env", fail)
    monkeypatch.setattr(hosts, "load_ex_hosts_from_file", lambda path: pytest.fail("invalid env cannot fall back"))
    with pytest.raises(ValueError, match="bad env"):
        hosts.load_ex_hosts()


def test_native_hosts_file_object_errors_preserve_cause_and_active_context(monkeypatch):
    events = []
    cause = OSError("read failed")

    class Target:
        def read_text(self, **kwargs):
            assert kwargs == {"encoding": "utf-8"}
            raise cause

        def __format__(self, spec):
            events.append(sys.exc_info()[1])
            return "target"

    monkeypatch.setattr(hosts, "Path", lambda path: Target())
    with pytest.raises(ValueError, match="无法读取扩展行情主机配置 target: read failed") as caught:
        hosts.load_ex_hosts_from_file("x")
    assert caught.value.__cause__ is cause and events == [cause]


def test_native_hosts_file_json_error_and_schema_errors_stay_distinct(tmp_path):
    target = tmp_path / "servers.json"
    assert hosts.load_ex_hosts_from_file(target) == []
    target.write_text("{", encoding="utf-8")
    with pytest.raises(ValueError, match="不是合法 JSON") as caught:
        hosts.load_ex_hosts_from_file(target)
    assert isinstance(caught.value.__cause__, json.JSONDecodeError)
    target.write_text("[]", encoding="utf-8")
    with pytest.raises(ValueError, match="顶层必须是对象"):
        hosts.load_ex_hosts_from_file(target)
    target.write_text('{"hosts": "wrong"}', encoding="utf-8")
    with pytest.raises(ValueError, match="缺少 hosts 数组"):
        hosts.load_ex_hosts_from_file(target)


def test_native_hosts_file_uses_live_parse_hook_after_json(monkeypatch):
    target = SimpleNamespace(read_text=lambda **kwargs: "raw")
    marker = object()
    seen = []
    monkeypatch.setattr(hosts, "Path", lambda path: target)

    def load(raw):
        assert raw == "raw"
        monkeypatch.setattr(hosts, "parse_ex_hosts", lambda values, *, source: seen.append((values, source)) or marker)
        return {"hosts": ["x"]}

    monkeypatch.setattr(hosts, "json", SimpleNamespace(loads=load))
    assert hosts.load_ex_hosts_from_file("path") is marker
    assert seen == [(["x"], str(target))]


def test_native_hosts_save_remains_explicit_with_utf8_schema_and_no_temporary_file(tmp_path):
    target = tmp_path / "subdir" / "servers.json"
    assert not target.exists()
    written = hosts.save_ex_hosts([" 中文.example ", "[::1]", "中文.example"], path=target)
    assert written == target
    content = target.read_text(encoding="utf-8")
    assert "中文" in content and content.endswith("\n")
    assert json.loads(content) == {"schema_version": 1, "hosts": ["中文.example", "[::1]"]}
    assert sorted(path.name for path in target.parent.iterdir()) == ["servers.json"]


def test_native_hosts_save_keeps_write_flush_fsync_and_replace_order(monkeypatch):
    events = []

    class Stream:
        def __enter__(self):
            events.append("enter")
            return self

        def __exit__(self, *args):
            events.append("exit")

        def write(self, text):
            events.append(("write", text))

        def flush(self):
            events.append("flush")

        def fileno(self):
            events.append("fileno")
            return 7

    temporary = SimpleNamespace(
        open=lambda *args, **kwargs: events.append(("open", args, kwargs)) or Stream(),
        chmod=lambda mode: events.append(("chmod", mode)),
        replace=lambda target: events.append(("replace", target)),
        unlink=lambda: events.append("unlink"),
    )
    target = SimpleNamespace(name="servers.json", parent=SimpleNamespace(mkdir=lambda **kwargs: events.append(("mkdir", kwargs))),
                             with_name=lambda name: events.append(("temporary", name)) or temporary)
    monkeypatch.setattr(hosts, "Path", lambda value: target)
    monkeypatch.setattr(hosts, "os", SimpleNamespace(getpid=lambda: 12, fsync=lambda fd: events.append(("fsync", fd))))
    monkeypatch.setattr(hosts, "get_ident", lambda: 34)
    monkeypatch.setattr(hosts, "json", SimpleNamespace(dump=lambda payload, stream, **kwargs: events.append(("dump", payload, kwargs))))
    assert hosts.save_ex_hosts(["x"], path="p") is target
    assert events == [("mkdir", {"parents": True, "exist_ok": True}), ("temporary", ".servers.json.12.34.tmp"),
                      ("open", ("w",), {"encoding": "utf-8"}), "enter", ("dump", {"schema_version": 1, "hosts": ["x"]}, {"ensure_ascii": False, "indent": 2}),
                      ("write", "\n"), "flush", "fileno", ("fsync", 7), "exit", ("chmod", 0o600), ("replace", target), "unlink"]


def test_native_hosts_empty_save_rejects_before_path_creation(monkeypatch):
    monkeypatch.setattr(hosts, "Path", lambda value: pytest.fail("empty input cannot create path"))
    with pytest.raises(ValueError, match="hosts 不能为空"):
        hosts.save_ex_hosts([], path="ignored")


def test_native_hosts_path_keeps_custom_division_and_live_filename(monkeypatch):
    class Directory:
        def __truediv__(self, name):
            return (self, name)

    directory = Directory()

    def root():
        monkeypatch.setattr(hosts, "EX_SERVER_FILE", "custom.json")
        return directory

    monkeypatch.setattr(hosts, "data_dir", root)
    assert hosts.ex_server_file_path() == (directory, "custom.json")
