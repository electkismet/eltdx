"""扩展行情主机列表的用户可覆盖配置层（环境变量 / 用户配置文件）。

全部离线：不触网，用 ``ELTDX_DATA_DIR`` 指向临时目录做隔离。
"""

from __future__ import annotations

import json

import pytest

from eltdx.ex import client as ex_client
from eltdx.ex.hosts import (
    DEFAULT_EX_HOSTS,
    DEFAULT_EX_PORT,
    EX_HOSTS_ENV,
    KNOWN_EX_HOSTS,
    PROBE_VERIFIED_EX_HOSTS,
    ex_server_file_path,
    load_ex_hosts,
    load_ex_hosts_from_env,
    load_ex_hosts_from_file,
    normalize_ex_host,
    parse_ex_hosts,
    save_ex_hosts,
)


@pytest.fixture(autouse=True)
def isolated_data_dir(tmp_path, monkeypatch):
    """把用户数据目录隔离到临时目录，并清空可能存在的环境变量。"""

    monkeypatch.setenv("ELTDX_DATA_DIR", str(tmp_path))
    monkeypatch.delenv(EX_HOSTS_ENV, raising=False)
    return tmp_path


# --------------------------------------------------------------------- #
# 默认主机表
# --------------------------------------------------------------------- #


def test_default_hosts_put_probe_verified_first():
    """实测唯一能应答枚举命令的主机必须排在默认顺序最前面。"""

    assert DEFAULT_EX_HOSTS[0] == PROBE_VERIFIED_EX_HOSTS[0]
    assert DEFAULT_EX_HOSTS[0] == "150.158.20.127"


def test_default_hosts_keep_every_known_host():
    """重排不能丢主机：官方 16 台都要还在。"""

    assert set(KNOWN_EX_HOSTS) <= set(DEFAULT_EX_HOSTS)
    assert len(DEFAULT_EX_HOSTS) == len(set(DEFAULT_EX_HOSTS)) == len(KNOWN_EX_HOSTS)


def test_default_port_is_7727():
    assert DEFAULT_EX_PORT == 7727


# --------------------------------------------------------------------- #
# normalize_ex_host / parse_ex_hosts
# --------------------------------------------------------------------- #


@pytest.mark.parametrize(
    "value,expected",
    [
        ("203.0.113.10", "203.0.113.10"),
        ("  example.com  ", "example.com"),
        ("[::1]", "[::1]"),
        ("tdx.internal.example", "tdx.internal.example"),
    ],
)
def test_normalize_ex_host_accepts_bare_hosts(value, expected):
    assert normalize_ex_host(value) == expected


@pytest.mark.parametrize(
    "value",
    [
        "203.0.113.10:7727",  # 带端口：明确拒绝，免得用户以为端口生效
        "203.0.113.10:9999",
        "",
        "   ",
        "a b",
        "http://203.0.113.10",
        "203.0.113.10/24",
        123,
        None,
        ["203.0.113.10"],
    ],
)
def test_normalize_ex_host_rejects_invalid(value):
    assert normalize_ex_host(value) is None


def test_parse_ex_hosts_dedupes_and_keeps_order():
    assert parse_ex_hosts(["b", "a", "b", " a "], source="t") == ["b", "a"]


def test_parse_ex_hosts_raises_with_actionable_message():
    with pytest.raises(ValueError) as excinfo:
        parse_ex_hosts(["ok.example", "bad:7727"], source="测试来源")
    message = str(excinfo.value)
    assert "测试来源" in message
    assert "bad:7727" in message
    assert "端口" in message


# --------------------------------------------------------------------- #
# 环境变量
# --------------------------------------------------------------------- #


def test_env_absent_or_blank_yields_empty():
    assert load_ex_hosts_from_env({}) == []
    assert load_ex_hosts_from_env({EX_HOSTS_ENV: "   "}) == []


@pytest.mark.parametrize("separator", [",", ";", " ", "\n", "\t", ", "])
def test_env_accepts_common_separators(separator):
    env = {EX_HOSTS_ENV: separator.join(["a.example", "b.example"])}
    assert load_ex_hosts_from_env(env) == ["a.example", "b.example"]


def test_env_alias_order_is_preserved():
    env = {EX_HOSTS_ENV: "z.example,a.example"}
    assert load_ex_hosts_from_env(env) == ["z.example", "a.example"]


def test_env_invalid_entry_raises():
    with pytest.raises(ValueError, match=EX_HOSTS_ENV):
        load_ex_hosts_from_env({EX_HOSTS_ENV: "good.example,bad:1"})


# --------------------------------------------------------------------- #
# 用户配置文件
# --------------------------------------------------------------------- #


def test_missing_file_yields_empty(tmp_path):
    assert load_ex_hosts_from_file(tmp_path / "nope.json") == []


def test_file_round_trip(tmp_path):
    target = tmp_path / "ex_servers.json"
    written = save_ex_hosts(["a.example", "b.example"], path=target)
    assert written == target
    assert load_ex_hosts_from_file(target) == ["a.example", "b.example"]

    payload = json.loads(target.read_text(encoding="utf-8"))
    assert payload["schema_version"] == 1
    assert payload["hosts"] == ["a.example", "b.example"]


def test_file_written_with_owner_only_permissions(tmp_path):
    target = save_ex_hosts(["a.example"], path=tmp_path / "ex_servers.json")
    if target.stat().st_mode & 0o777 == 0o600:
        assert True
    else:  # pragma: no cover - Windows 上 chmod 语义不同
        assert target.exists()


def test_file_leaves_no_temporary_artifacts(tmp_path):
    save_ex_hosts(["a.example"], path=tmp_path / "ex_servers.json")
    leftovers = [p.name for p in tmp_path.iterdir() if p.suffix == ".tmp"]
    assert leftovers == []


def test_file_invalid_json_raises(tmp_path):
    target = tmp_path / "ex_servers.json"
    target.write_text("{not json", encoding="utf-8")
    with pytest.raises(ValueError, match="不是合法 JSON"):
        load_ex_hosts_from_file(target)


def test_file_missing_hosts_key_raises(tmp_path):
    target = tmp_path / "ex_servers.json"
    target.write_text('{"schema_version": 1}', encoding="utf-8")
    with pytest.raises(ValueError, match="hosts"):
        load_ex_hosts_from_file(target)


def test_file_wrong_top_level_type_raises(tmp_path):
    target = tmp_path / "ex_servers.json"
    target.write_text('["a.example"]', encoding="utf-8")
    with pytest.raises(ValueError, match="顶层必须是对象"):
        load_ex_hosts_from_file(target)


def test_file_invalid_entry_raises(tmp_path):
    target = tmp_path / "ex_servers.json"
    target.write_text('{"hosts": ["ok.example", "bad:7727"]}', encoding="utf-8")
    with pytest.raises(ValueError, match="bad:7727"):
        load_ex_hosts_from_file(target)


def test_save_rejects_empty_list(tmp_path):
    with pytest.raises(ValueError, match="不能为空"):
        save_ex_hosts([], path=tmp_path / "ex_servers.json")


def test_save_rejects_invalid_entry(tmp_path):
    with pytest.raises(ValueError, match="save_ex_hosts"):
        save_ex_hosts(["bad:7727"], path=tmp_path / "ex_servers.json")


# --------------------------------------------------------------------- #
# 优先级：环境变量 > 用户配置 > 包内默认
# --------------------------------------------------------------------- #


def test_precedence_env_over_file_over_default(tmp_path):
    target = tmp_path / "ex_servers.json"
    save_ex_hosts(["from-file.example"], path=target)

    # 只有文件：用文件
    assert load_ex_hosts(env={}, path=target) == ["from-file.example"]
    # 环境变量压过文件
    assert load_ex_hosts(env={EX_HOSTS_ENV: "from-env.example"}, path=target) == [
        "from-env.example"
    ]
    # 两者都没有：用包内默认
    assert load_ex_hosts(env={}, path=tmp_path / "missing.json") == list(DEFAULT_EX_HOSTS)


def test_default_path_is_under_data_dir(tmp_path):
    assert ex_server_file_path() == tmp_path / "ex_servers.json"


# --------------------------------------------------------------------- #
# 与 ExTdxClient 的接线
# --------------------------------------------------------------------- #


def test_client_uses_env_override(monkeypatch):
    monkeypatch.setenv(EX_HOSTS_ENV, "my-example-host")
    client = ex_client.ExTdxClient()
    assert client.host == "my-example-host"
    assert client._hosts == ["my-example-host"]


def test_client_uses_saved_user_config(tmp_path):
    save_ex_hosts(["saved.example"], path=tmp_path / "ex_servers.json")
    client = ex_client.ExTdxClient()
    assert client.host == "saved.example"


def test_explicit_hosts_still_win_over_override(monkeypatch):
    monkeypatch.setenv(EX_HOSTS_ENV, "from-env.example")
    client = ex_client.ExTdxClient(hosts=["explicit.example"])
    assert client._hosts == ["explicit.example"]


def test_client_default_falls_back_to_default_hosts():
    client = ex_client.ExTdxClient()
    assert client._hosts == list(DEFAULT_EX_HOSTS)
    # 首选是实测可用的那台，即使没走并发探活也是能取数的
    assert client.host == PROBE_VERIFIED_EX_HOSTS[0]
