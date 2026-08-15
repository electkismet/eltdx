"""TDX server file models."""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class FileContentChunk:
    path: str
    offset: int
    request_size: int
    chunk_len: int
    content: bytes
    raw_payload: bytes = b""

    @property
    def is_last(self) -> bool:
        return self.chunk_len < self.request_size


@dataclass(frozen=True, slots=True)
class TdxStatRow:
    market_id: int
    code: str
    stats_date: str | None
    beta_60d: float | None
    pe_ttm: float | None
    free_float_shares_10k: float | None
    year_limit_up_days: int | None
    limit_stat_days: int | None
    limit_up_count_in_stat_days: int | None
    limit_up_streak_days: int | None

    @property
    def key(self) -> tuple[int, str]:
        return self.market_id, self.code


@dataclass(frozen=True, slots=True)
class TdxStat2Row:
    market_id: int
    code: str
    stats_date: str | None
    amount_10k: float | None
    seal_amount_10k: float | None
    prev_amount_10k: float | None
    prev_seal_amount_10k: float | None
    prev2_amount_10k: float | None
    prev2_seal_amount_10k: float | None
    open_volume_hand: float | None
    prev_open_volume_hand: float | None
    open_amount_10k: float | None
    prev_open_amount_10k: float | None

    @property
    def key(self) -> tuple[int, str]:
        return self.market_id, self.code


@dataclass(frozen=True, slots=True)
class TdxStatsResource:
    stat: dict[tuple[int, str], TdxStatRow]
    stat2: dict[tuple[int, str], TdxStat2Row]
    source_path: str

    def row(self, market_id: int, code: str) -> tuple[TdxStatRow | None, TdxStat2Row | None]:
        key = int(market_id), str(code).zfill(6)
        return self.stat.get(key), self.stat2.get(key)

    @property
    def stat_count(self) -> int:
        return len(self.stat)

    @property
    def stat2_count(self) -> int:
        return len(self.stat2)

    @property
    def stats_date(self) -> str | None:
        counts = self.stats_date_counts
        if not counts:
            return None
        return max(counts, key=lambda value: (counts[value], value))

    @property
    def stats_date_counts(self) -> dict[str, int]:
        values: list[str] = []
        for stat_row in self.stat.values():
            if stat_row.stats_date:
                values.append(stat_row.stats_date)
        for stat2_row in self.stat2.values():
            if stat2_row.stats_date:
                values.append(stat2_row.stats_date)
        return dict(Counter(values))

    @property
    def stats_date_coverage(self) -> float:
        total = self.stat_count + self.stat2_count
        if total == 0 or self.stats_date is None:
            return 0.0
        return self.stats_date_counts[self.stats_date] / total
