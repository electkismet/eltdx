"""板块资料准备、解析和按需行情组合。"""

from __future__ import annotations

import hashlib
import json
import os
import re
import struct
import weakref
from dataclasses import dataclass
from datetime import date, datetime, timezone
from pathlib import Path
from typing import TYPE_CHECKING, Any

from eltdx.protocol.unit import ID_TO_MARKET, MARKET_TO_ID

if TYPE_CHECKING:
    from eltdx.client import TdxClient


BOARD_BATCH_SIZE = 80
BOARD_FILES = ("infoharbor_block.dat", "tdxhy.cfg")
DEFINITION_FILES = ("tdxzs.cfg", "tdxzs3.cfg")
BOARD_CATEGORIES = ("概念", "风格", "地区", "一级行业", "二级行业", "三级行业", "880行业", "全部")


@dataclass(frozen=True, slots=True)
class BoardQuoteRow:
    board_code: str
    board_name: str
    full_code: str
    exchange: str
    market_id: int
    last_price: float | None
    pre_close_price: float | None
    open_price: float | None
    high_price: float | None
    low_price: float | None
    change: float | None
    change_pct: float | None
    amount: float | None
    volume_hand: int | None
    quote: Any | None = None

    @property
    def code(self) -> str:
        return self.full_code[2:]

    @property
    def name(self) -> str:
        return self.board_name


@dataclass(frozen=True, slots=True)
class BoardQuoteTable:
    rows: tuple[BoardQuoteRow, ...]
    prepared_date: date

    @property
    def count(self) -> int:
        return len(self.rows)


@dataclass(frozen=True, slots=True)
class BoardMemberQuoteTable:
    board_code: str
    board_name: str
    raw_members: tuple[dict[str, Any], ...]
    display_members: tuple[dict[str, Any], ...]
    excluded_members: tuple[dict[str, Any], ...]
    rows: tuple[BoardQuoteRow, ...]
    prepared_date: date

    @property
    def count(self) -> int:
        return len(self.rows)


class BoardService:
    """Compose board definition files, 0x044d and 0x054c.

    The service is deliberately lazy.  A successful preparation is recorded
    on disk by date, so later calls on the same day reuse both files and the
    current security table.  ``refresh=True`` is the explicit escape hatch.
    """

    def __init__(self, client: Any, *, data_dir: str | os.PathLike[str] | None = None,
                 definitions_dir: str | os.PathLike[str] | None = None) -> None:
        # 只存弱引用：BoardService 由 HelperApi/client 持有，强引用 client 会
        # 构成引用环，让 client（以及底层 ~5 条 worker 线程）只能等 GC 回收。
        self._client_ref = weakref.ref(client)
        selected_data_dir: str | os.PathLike[str] = data_dir or os.environ.get(
            "ELTDX_BOARD_DATA_DIR", str(Path.cwd() / "downloads")
        )
        self._data_dir = Path(selected_data_dir).expanduser()
        self._definitions_dir = Path(definitions_dir).expanduser() if definitions_dir else None
        self._prepared_date: date | None = None
        self._boards: tuple[dict[str, Any], ...] | None = None
        self._members: dict[str, dict[str, Any]] = {}
        self._security: dict[tuple[int, str], Any] | None = None

    @property
    def _client(self) -> TdxClient:
        """解析 TdxClient 弱引用；client 已回收时明确报错而不是静默降级。"""

        client = self._client_ref()
        if client is None:
            raise ReferenceError("TdxClient 已被回收")
        return client

    def clear_cache(self) -> None:
        self._prepared_date = None
        self._boards = None
        self._members.clear()
        self._security = None

    def board_quotes(self, *, category: str = "概念", refresh: bool = False) -> BoardQuoteTable:
        from eltdx.transport.native import native_module

        return native_module().board_quotes(self, category, refresh, globals())

    def board_member_quotes(self, board_code: str, *, refresh: bool = False) -> BoardMemberQuoteTable:
        from eltdx.transport.native import native_module

        return native_module().board_member_quotes(self, board_code, refresh, globals())

    def _prepare(self, *, refresh: bool) -> dict[str, Any]:
        from eltdx.transport.native import native_module

        return native_module().board_prepare(self, refresh, globals())

    def _download_missing_or_refresh(self, *, refresh: bool) -> None:
        from eltdx.transport.native import native_module

        native_module().board_download(self, refresh, BOARD_FILES)

    def _cache_files_ready(self) -> bool:
        from eltdx.transport.native import native_module

        return native_module().board_cache_ready(self, BOARD_FILES)

    def _definition_paths(self) -> list[Path]:
        from eltdx.transport.native import native_module

        return native_module().board_definition_paths(self, globals())

    def _parse_boards(self) -> list[dict[str, Any]]:
        from eltdx.transport.native import native_module

        return native_module().board_parse(self, globals())

    def _members_for(self, board: dict[str, Any]) -> dict[str, Any]:
        from eltdx.transport.native import native_module

        return native_module().board_members(self, board, globals())

    def _load_security_from_044d(self) -> dict[tuple[int, str], Any]:
        from eltdx.transport.native import native_module

        return native_module().board_security(self, MARKET_TO_ID)

    def _load_security_file(self) -> dict[tuple[int, str], Any] | None:
        from eltdx.transport.native import native_module

        return native_module().board_read_security(self, json)

    def _write_security_file(self, security: dict[tuple[int, str], Any]) -> None:
        from eltdx.transport.native import native_module

        native_module().board_write_security(self, security, json)

    def _snapshot_batches(self, codes: list[str]) -> list[Any]:
        from eltdx.transport.native import native_module

        return native_module().board_snapshot_batches(self, codes, BOARD_BATCH_SIZE)

    @staticmethod
    def _quote_row(board: dict[str, Any], quote: Any | None) -> BoardQuoteRow:
        from eltdx.transport.native import native_module

        return native_module().board_quote_row(board, quote, globals())

    def _market_date(self) -> date:
        from eltdx.transport.native import native_module

        return native_module().board_date(self, date)

    @staticmethod
    def _full_security_code(market: int, code: str) -> str:
        from eltdx.transport.native import native_module

        return native_module().board_full_code(market, code, ID_TO_MARKET)


def _read_text(path: Path) -> str:
    from eltdx.transport.native import native_module

    return native_module().board_read_text(path)


def _read_json(path: Path) -> dict[str, Any]:
    from eltdx.transport.native import native_module

    return native_module().board_read_json(path, json)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _board_market(code: str, declared: str | None) -> str:
    from eltdx.transport.native import native_module

    return native_module().board_market(code, declared, ID_TO_MARKET)


def _matches_category(board: dict[str, Any], category: str) -> bool:
    from eltdx.transport.native import native_module

    return native_module().board_matches(board, category, re)


def _infoharbor_headers(path: Path) -> list[tuple[str, str, int]]:
    from eltdx.transport.native import native_module

    return native_module().board_infoharbor(path, globals())


def _read_dbf_members(path: Path, region_key: str) -> set[tuple[int, str]]:
    from eltdx.transport.native import native_module

    return native_module().board_dbf_members(path, region_key, globals())
