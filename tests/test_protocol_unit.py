"""``eltdx.protocol.unit`` 的代码/市场规范化测试（纯离线）。

重点覆盖 5xxxxx 号段：缺失时沪市 ETF/LOF/货币基金会被判为
``unable to infer market``，7709 查询直接失败。
"""

from __future__ import annotations

import pytest

from eltdx.exceptions import ProtocolError
from eltdx.protocol.unit import (
    MARKET_TO_ID,
    market_to_id,
    normalize_code,
    normalize_market,
    split_code,
)


@pytest.mark.parametrize(
    ("code", "expected"),
    [
        # 沪市 A 股 / 科创板
        ("600000", "sh600000"),
        ("601398", "sh601398"),
        ("688981", "sh688981"),
        # 沪市基金 / ETF / LOF / 货币基金 / 权证（本轮补全的号段）
        ("510300", "sh510300"),
        ("511360", "sh511360"),
        ("512880", "sh512880"),
        ("513180", "sh513180"),
        ("515180", "sh515180"),
        ("518850", "sh518850"),
        ("501050", "sh501050"),
        ("502050", "sh502050"),
        ("580000", "sh580000"),
        # 沪市 B 股
        ("900901", "sh900901"),
        # 深市主板 / 中小板 / 创业板
        ("000001", "sz000001"),
        ("002415", "sz002415"),
        ("300750", "sz300750"),
        # 深市基金 / 债券
        ("159915", "sz159915"),
        ("128035", "sz128035"),
        ("123456", "sz123456"),
        # 深市 B 股
        ("200011", "sz200011"),
        # 北交所：4xxxxx / 8xxxxx / 920xxx
        ("430047", "bj430047"),
        ("830799", "bj830799"),
        ("871981", "bj871981"),
        ("920001", "bj920001"),
    ],
)
def test_normalize_code_infers_market_by_segment(code: str, expected: str) -> None:
    assert normalize_code(code) == expected


def test_five_prefix_is_shanghai_funds() -> None:
    """5xxxxx 必须是沪市，而不是「无法推断」。"""

    for code in ("510300", "511360", "515180", "518850", "588000"):
        assert normalize_code(code) == "sh" + code


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("sz000001", "sz000001"),
        ("SH600000", "sh600000"),
        ("BJ430047", "bj430047"),
        ("  sz000001  ", "sz000001"),
    ],
)
def test_normalize_code_accepts_prefixed_form(value: str, expected: str) -> None:
    assert normalize_code(value) == expected


@pytest.mark.parametrize("code", ["", "60000", "6000000", "abcdef", "12345a", "700000"])
def test_normalize_code_rejects_invalid(code: str) -> None:
    with pytest.raises(ProtocolError):
        normalize_code(code)


def test_split_code_returns_market_id_market_and_number() -> None:
    assert split_code("600000") == (1, "sh", "600000")
    assert split_code("000001") == (0, "sz", "000001")
    assert split_code("430047") == (2, "bj", "430047")
    assert split_code("510300") == (1, "sh", "510300")


def test_market_aliases_and_ids() -> None:
    assert MARKET_TO_ID == {"sz": 0, "sh": 1, "bj": 2}
    for value in ("sz", "SZ", "0", "sza", "深市"):
        assert normalize_market(value) == "sz"
    for value in ("sh", "SH", "1", "sha", "沪市"):
        assert normalize_market(value) == "sh"
    for value in ("bj", "BJ", "2", "bse", "北交所"):
        assert normalize_market(value) == "bj"
    assert normalize_market(0) == "sz"
    assert market_to_id("sh") == 1


@pytest.mark.parametrize("value", ["xx", 9, ""])
def test_market_rejects_unknown(value: object) -> None:
    with pytest.raises(ProtocolError):
        normalize_market(value)  # type: ignore[arg-type]


def test_market_enum_matches_string_mapping() -> None:
    """``Market`` 枚举（easy_tdx 兼容）必须与 ``MARKET_TO_ID`` 一致，防止漂移。"""

    from eltdx.models import Market

    assert {m.code for m in Market} == set(MARKET_TO_ID)
    for member in Market:
        assert MARKET_TO_ID[member.code] == int(member)
        # 枚举与字符串可互换传给 normalize_market / market_to_id
        assert normalize_market(member) == member.code
        assert market_to_id(member.code) == int(member)
        assert market_to_id(member) == int(member)
