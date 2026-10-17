"""获取扩展行情商品列表行情（按类别）。"""

from __future__ import annotations

import struct
from collections import OrderedDict

from eltdx.ex.commands.base import BaseCommand


class GetExInstrumentQuoteListCmd(BaseCommand[list[OrderedDict[str, object]]]):
    """按类别获取商品行情列表（期货/港股等）。"""

    def __init__(
        self,
        market: int,
        category: int,
        start: int = 0,
        count: int = 80,
    ) -> None:
        if category not in (2, 3):
            raise ValueError(f"不支持的扩展行情类别: {category}")
        if start < 0:
            raise ValueError(f"start 必须 >= 0，当前: {start}")
        if count <= 0:
            raise ValueError(f"count 必须 > 0，当前: {count}")
        self.market = market
        self.category = category
        self.start = start
        self.count = count

    def build_request(self) -> bytes:
        """构造请求包。"""

        header = bytes.fromhex("01 c1 06 0b 00 02 0b 00 0b 00 00 24")
        return header + struct.pack(
            "<BHHHH",
            self.market,
            0,
            self.start,
            self.count,
            1,
        )

    def parse_response(self, body: bytes) -> list[OrderedDict[str, object]]:
        """解析商品列表行情响应。"""

        if len(body) < 2:
            return []
        (num,) = struct.unpack("<H", body[0:2])
        pos = 2
        results: list[OrderedDict[str, object]] = []
        for _ in range(num):
            if pos + 10 > len(body):
                break
            (market, raw_code) = struct.unpack("<B9s", body[pos : pos + 10])
            code = raw_code.strip(b"\x00").decode("gbk", errors="replace")
            pos += 10
            if self.category == 3:
                pos = self._parse_futures(market, code, body, pos, results)
            else:
                pos = self._parse_hk_stocks(market, code, body, pos, results)
        return results

    @staticmethod
    def _parse_futures(
        market: int,
        code: str,
        body: bytes,
        pos: int,
        results: list[OrderedDict[str, object]],
    ) -> int:
        if pos + 140 > len(body):
            return pos + 290
        (
            bi_shu,
            zuo_jie,
            jin_kai,
            zui_gao,
            zui_di,
            mai_chu,
            kai_cang,
            _unk1,
            zong_liang,
            xian_liang,
            zong_jin_e,
            nei_pan,
            wai_pan,
            _unk2,
            chi_cang_liang,
            mai_ru_jia,
            _u1,
            _u2,
            _u3,
            _u4,
            mai_ru_liang,
            _u5,
            _u6,
            _u7,
            _u8,
            mai_chu_jia,
            _u9,
            _u10,
            _u11,
            _u12,
            mai_chu_liang,
            _u13,
            _u14,
            _u15,
        ) = struct.unpack("<IfffffIIIIfIIfIfIIIIIIIIIfIIIIIIIII", body[pos : pos + 140])
        pos += 290
        results.append(
            OrderedDict(
                [
                    ("market", market),
                    ("code", code),
                    ("BiShu", bi_shu),
                    ("ZuoJie", zuo_jie),
                    ("JinKai", jin_kai),
                    ("ZuiGao", zui_gao),
                    ("ZuiDi", zui_di),
                    ("MaiChu", mai_chu),
                    ("KaiCang", kai_cang),
                    ("ZongLiang", zong_liang),
                    ("XianLiang", xian_liang),
                    ("ZongJinE", zong_jin_e),
                    ("NeiPan", nei_pan),
                    ("WaiPan", wai_pan),
                    ("ChiCangLiang", chi_cang_liang),
                    ("MaiRuJia", mai_ru_jia),
                    ("MaiRuLiang", mai_ru_liang),
                    ("MaiChuJia", mai_chu_jia),
                    ("MaiChuLiang", mai_chu_liang),
                ]
            )
        )
        return pos

    @staticmethod
    def _parse_hk_stocks(
        market: int,
        code: str,
        body: bytes,
        pos: int,
        results: list[OrderedDict[str, object]],
    ) -> int:
        if pos + 140 > len(body):
            return pos + 290
        (
            huo_yue_du,
            zuo_shou,
            jin_kai,
            zui_gao,
            zui_di,
            xian_jia,
            _unk1,
            mai_ru_jia,
            zong_liang,
            xian_liang,
            zong_jin_e,
            _unk2,
            _unk3,
            nei,
            wai,
            mrj1,
            mrj2,
            mrj3,
            mrj4,
            mrj5,
            mrl1,
            mrl2,
            mrl3,
            mrl4,
            mrl5,
            mcj1,
            mcj2,
            mcj3,
            mcj4,
            mcj5,
            mcl1,
            mcl2,
            mcl3,
            mcl4,
            mcl5,
        ) = struct.unpack("<IfffffIfIIfIIIIfffffIIIIIfffffIIIII", body[pos : pos + 140])
        pos += 290
        results.append(
            OrderedDict(
                [
                    ("market", market),
                    ("code", code),
                    ("HuoYueDu", huo_yue_du),
                    ("ZuoShou", zuo_shou),
                    ("JinKai", jin_kai),
                    ("ZuiGao", zui_gao),
                    ("ZuiDi", zui_di),
                    ("XianJia", xian_jia),
                    ("MaiRuJia", mai_ru_jia),
                    ("ZongLiang", zong_liang),
                    ("XianLiang", xian_liang),
                    ("ZongJinE", zong_jin_e),
                    ("Nei", nei),
                    ("Wai", wai),
                    ("MaiRuJia1", mrj1),
                    ("MaiRuJia2", mrj2),
                    ("MaiRuJia3", mrj3),
                    ("MaiRuJia4", mrj4),
                    ("MaiRuJia5", mrj5),
                    ("MaiRuLiang1", mrl1),
                    ("MaiRuLiang2", mrl2),
                    ("MaiRuLiang3", mrl3),
                    ("MaiRuLiang4", mrl4),
                    ("MaiRuLiang5", mrl5),
                    ("MaiChuJia1", mcj1),
                    ("MaiChuJia2", mcj2),
                    ("MaiChuJia3", mcj3),
                    ("MaiChuJia4", mcj4),
                    ("MaiChuJia5", mcj5),
                    ("MaiChuLiang1", mcl1),
                    ("MaiChuLiang2", mcl2),
                    ("MaiChuLiang3", mcl3),
                    ("MaiChuLiang4", mcl4),
                    ("MaiChuLiang5", mcl5),
                ]
            )
        )
        return pos


__all__ = ["GetExInstrumentQuoteListCmd"]
