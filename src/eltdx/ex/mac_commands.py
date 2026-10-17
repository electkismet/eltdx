"""MAC-EX 复用的 MAC 命令（**薄转发层**）。

单一事实来源在 :mod:`eltdx.mac.commands`；本模块只做再导出，理由与
:mod:`eltdx.ex.mac_models` 相同——同一批命令不应有两份定义。

.. warning::
   本模块**曾经**复制过一份 ``mac/commands.py`` 的命令类。两份副本随后各自
   漂移，并因此产生过真实缺陷：``SymbolBarCmd`` 的帧体少 1 个标志字节
   （``bbb`` 而非 ``bbbb``），服务端对任何标的都只回 1 字节空体，导致
   ``MacClient.get_stock_kline`` 全线抛 ``ProtocolError``。合并为单一来源后
   该类问题不会再出现——**请勿在此重新定义命令类**。
"""

from __future__ import annotations

from eltdx.mac.bitmap import FieldBit
from eltdx.mac.commands import (
    ChartSamplingCmd,
    GoodsListCmd,
    SymbolBarCmd,
    SymbolQuotesCmd,
    SymbolTickChartCmd,
    SymbolTransactionCmd,
)
from eltdx.mac.enums import Adjust, Period

#: :meth:`eltdx.ex.MacExClient.goods_quotes` 的默认请求字段。
#:
#: 与 :data:`eltdx.mac.bitmap.PresetField.COMMON`（34 个字段）不同：MAC-EX 的
#: 批量报价历史上只请求 7 个基础字段，这里保持字节稳定（勿改顺序/成员）。
DEFAULT_QUOTE_FIELDS: tuple[FieldBit, ...] = (
    FieldBit.PRE_CLOSE,
    FieldBit.OPEN,
    FieldBit.HIGH,
    FieldBit.LOW,
    FieldBit.CLOSE,
    FieldBit.VOL,
    FieldBit.AMOUNT,
)

__all__ = [
    "DEFAULT_QUOTE_FIELDS",
    "Adjust",
    "ChartSamplingCmd",
    "GoodsListCmd",
    "Period",
    "SymbolBarCmd",
    "SymbolQuotesCmd",
    "SymbolTickChartCmd",
    "SymbolTransactionCmd",
]
