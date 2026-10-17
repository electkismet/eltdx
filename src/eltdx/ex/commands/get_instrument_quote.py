"""获取扩展行情实时五档报价。"""

from __future__ import annotations

import struct

from eltdx.ex.commands.base import BaseCommand
from eltdx.ex.models import ExInstrumentQuote


class GetExInstrumentQuoteCmd(BaseCommand[ExInstrumentQuote | None]):
    """获取单个商品的五档实时行情。"""

    def __init__(self, market: int, code: str) -> None:
        if not code:
            raise ValueError("code 不能为空")
        self.market = market
        self.code = code.encode("utf-8")

    def build_request(self) -> bytes:
        """构造请求包。"""

        header = bytes.fromhex("01 01 08 02 02 01 0c 00 0c 00 fa 23")
        return header + struct.pack("<B9s", self.market, self.code)

    def parse_response(self, body: bytes) -> ExInstrumentQuote | None:
        """解析五档报价响应。"""

        if len(body) < 150:
            return None
        pos = 0
        (market, raw_code) = struct.unpack("<B9s", body[pos : pos + 10])
        pos += 10
        pos += 4  # skip 4 unknown bytes
        (
            pre_close,
            open_price,
            high,
            low,
            price,
            kaicang,
            _unk1,
            zongliang,
            xianliang,
            _unk2,
            neipan,
            waipan,
            _unk3,
            chicang,
            b1,
            b2,
            b3,
            b4,
            b5,
            bv1,
            bv2,
            bv3,
            bv4,
            bv5,
            a1,
            a2,
            a3,
            a4,
            a5,
            av1,
            av2,
            av3,
            av4,
            av5,
        ) = struct.unpack(
            "<fffffIIIIIIIIIfffffIIIIIfffffIIIII",
            body[pos : pos + 136],
        )
        code = raw_code.decode("utf-8", errors="replace").rstrip("\x00")
        return ExInstrumentQuote(
            market=market,
            code=code,
            pre_close=pre_close,
            open=open_price,
            high=high,
            low=low,
            price=price,
            kaicang=kaicang,
            zongliang=zongliang,
            xianliang=xianliang,
            neipan=neipan,
            waipan=waipan,
            chicang=chicang,
            bid1=b1,
            bid2=b2,
            bid3=b3,
            bid4=b4,
            bid5=b5,
            bid_vol1=bv1,
            bid_vol2=bv2,
            bid_vol3=bv3,
            bid_vol4=bv4,
            bid_vol5=bv5,
            ask1=a1,
            ask2=a2,
            ask3=a3,
            ask4=a4,
            ask5=a5,
            ask_vol1=av1,
            ask_vol2=av2,
            ask_vol3=av3,
            ask_vol4=av4,
            ask_vol5=av5,
            _raw=body[0 : pos + 136],
        )


__all__ = ["GetExInstrumentQuoteCmd"]
