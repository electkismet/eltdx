"""`normalize_host` / `unique_hosts` 的单元测试。

补默认端口（7709）是这次的行为变更：以前 ``"116.205.183.150"`` 这类省略端口的
主机会被 `normalize_host` 静默丢弃，用户最终只看到一句难以定位的
``at least one host is required``。这个函数此前**完全没有测试覆盖**，所以
``TdxClient(host="116.205.183.150")`` 一直是坏的。
"""

from __future__ import annotations

import pytest

from eltdx.hosts import DEFAULT_QUOTE_PORT, normalize_host, unique_hosts


def test_default_quote_port_is_7709() -> None:
    assert DEFAULT_QUOTE_PORT == 7709


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        # 裸 IP / 裸域名：补默认端口（本轮修复的核心）
        ("116.205.183.150", "116.205.183.150:7709"),
        ("example.com", "example.com:7709"),
        (" 116.205.183.150 ", "116.205.183.150:7709"),
        ("host:", "host:7709"),
        # 显式端口必须原样保留，不能被默认值覆盖
        ("116.205.183.150:7709", "116.205.183.150:7709"),
        ("116.205.183.150:7727", "116.205.183.150:7727"),
        # IPv6：裸写法要补方括号，否则拼出 "::1:7709" 之后无法再解析回来
        ("::1", "[::1]:7709"),
        ("[::1]", "[::1]:7709"),
        ("[::1]:7709", "[::1]:7709"),
        ("2001:db8::1", "[2001:db8::1]:7709"),
        ("::ffff:1.2.3.4", "[::ffff:1.2.3.4]:7709"),
    ],
)
def test_normalize_host_fills_in_default_port(raw: object, expected: str) -> None:
    assert normalize_host(raw) == expected


@pytest.mark.parametrize(
    "raw",
    ["", "   ", "host:abc", "host:0", "host:65536", "host:-1", "[::1", "a:b", None, 123],
)
def test_normalize_host_rejects_invalid_input(raw: object) -> None:
    assert normalize_host(raw) is None


def test_normalize_host_requires_explicit_port_when_default_disabled() -> None:
    assert normalize_host("1.2.3.4", default_port=None) is None
    assert normalize_host("1.2.3.4:7709", default_port=None) == "1.2.3.4:7709"


@pytest.mark.parametrize(
    "raw", ["116.205.183.150", "::1", "[::1]:7709", "example.com", "1.2.3.4:7727"]
)
def test_normalize_host_is_idempotent(raw: str) -> None:
    once = normalize_host(raw)
    assert once is not None
    assert normalize_host(once) == once


def test_unique_hosts_dedupes_bare_ip_against_explicit_port() -> None:
    # 同一个主机的两种写法必须归一成一条，否则会被重复连接
    assert unique_hosts(["116.205.183.150", "116.205.183.150:7709"]) == [
        "116.205.183.150:7709"
    ]


def test_unique_hosts_drops_unparsable_entries() -> None:
    assert unique_hosts(["116.205.183.150", "  ", None, "::1"]) == [
        "116.205.183.150:7709",
        "[::1]:7709",
    ]
