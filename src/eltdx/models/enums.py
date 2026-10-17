"""市场枚举（与 ``easy_tdx.models.enums`` 同名同路径）。

``eltdx`` 内部一律使用小写字符串 ``"sz"`` / ``"sh"`` / ``"bj"``
（见 :func:`eltdx.protocol.unit.normalize_market`）。本枚举提供给从
``easy_tdx`` 迁移的用户：``Market.SH`` 与字符串 ``"sh"`` 等价（取值相同），
两者可互换传给任何接受 ``market`` 的接口。
"""

from __future__ import annotations

from enum import IntEnum


class Market(IntEnum):
    """7709 标准行情的市场编号。

    | 成员 | 值 | 字符串 | 交易所 |
    | --- | --- | --- | --- |
    | :attr:`SZ` | 0 | ``"sz"`` | 深圳证券交易所 |
    | :attr:`SH` | 1 | ``"sh"`` | 上海证券交易所 |
    | :attr:`BJ` | 2 | ``"bj"`` | 北京证券交易所 |
    """

    SZ = 0
    SH = 1
    BJ = 2

    @property
    def code(self) -> str:
        """小写字符串形式（``"sz"`` / ``"sh"`` / ``"bj"``）。"""

        return _ID_TO_MARKET[int(self)]


#: 市场编号 -> 小写字符串。与 :data:`eltdx.protocol.unit.MARKET_TO_ID` 互为逆映射，
#: 二者一致性由 ``tests/test_protocol_unit.py`` 断言，防止漂移。
_ID_TO_MARKET: dict[int, str] = {
    Market.SZ: "sz",
    Market.SH: "sh",
    Market.BJ: "bj",
}

__all__ = ["Market"]
