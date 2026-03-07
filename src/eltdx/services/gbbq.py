from __future__ import annotations

from dataclasses import dataclass, field

from ..adjustment import apply_factors_to_kline, build_factor_response
from ..client import TdxClient
from ..equity import compute_turnover, filter_equity_items, pick_equity
from ..models import EquityResponse, FactorResponse, GbbqItem, GbbqResponse, KlineResponse, XdxrItem
from ..protocol.model_gbbq import filter_xdxr_items
from ..protocol.unit import add_prefix


@dataclass(slots=True)
class GbbqService:
    client: TdxClient
    _responses: dict[str, GbbqResponse] = field(default_factory=dict, init=False, repr=False)
    _factors: dict[str, FactorResponse] = field(default_factory=dict, init=False, repr=False)

    def refresh(self, code: str, *, include_raw: bool = False) -> GbbqResponse:
        full_code = add_prefix(code)
        response = self.client.get_gbbq(full_code, include_raw=include_raw)
        self._responses[full_code] = response
        self._factors.pop(full_code, None)
        return response

    def clear(self, code: str | None = None) -> None:
        if code is None:
            self._responses.clear()
            self._factors.clear()
            return

        full_code = add_prefix(code)
        self._responses.pop(full_code, None)
        self._factors.pop(full_code, None)

    def get_gbbq(self, code: str, *, refresh: bool = False, include_raw: bool = False) -> GbbqResponse:
        full_code = add_prefix(code)
        if refresh or full_code not in self._responses:
            return self.refresh(full_code, include_raw=include_raw)
        return self._responses[full_code]

    def items(self, code: str, *, refresh: bool = False) -> list[GbbqItem]:
        return list(self.get_gbbq(code, refresh=refresh).items)

    def get_xdxr(self, code: str, *, refresh: bool = False) -> list[XdxrItem]:
        return filter_xdxr_items(self.get_gbbq(code, refresh=refresh).items)

    def get_equity_changes(self, code: str, *, refresh: bool = False) -> EquityResponse:
        return filter_equity_items(self.get_gbbq(code, refresh=refresh).items)

    def get_equity(self, code: str, on=None, *, refresh: bool = False):
        return pick_equity(self.get_equity_changes(code, refresh=refresh).items, on)

    def get_turnover(self, code: str, volume: int | float, *, on=None, unit: str = "hand", refresh: bool = False) -> float:
        return compute_turnover(self.get_equity(code, on=on, refresh=refresh), volume, unit=unit)

    def get_factors(self, code: str, *, refresh: bool = False) -> FactorResponse:
        full_code = add_prefix(code)
        if refresh or full_code not in self._factors:
            day_kline = self.client.get_kline_all(full_code, "day")
            factors = build_factor_response(day_kline, self.get_xdxr(full_code, refresh=refresh))
            self._factors[full_code] = factors
        return self._factors[full_code]

    def get_adjusted_kline(
        self,
        code: str,
        freq: str,
        *,
        adjust: str = "qfq",
        start: int = 0,
        count: int = 800,
        include_raw: bool = False,
        refresh: bool = False,
    ) -> KlineResponse:
        response = self.client.get_kline(code, freq, start=start, count=count, include_raw=include_raw)
        return apply_factors_to_kline(response, self.get_factors(code, refresh=refresh), adjust)

    def get_adjusted_kline_all(self, code: str, freq: str, *, adjust: str = "qfq", refresh: bool = False) -> KlineResponse:
        response = self.client.get_kline_all(code, freq)
        return apply_factors_to_kline(response, self.get_factors(code, refresh=refresh), adjust)
