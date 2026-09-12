"""板块资料准备、解析和按需行情组合。"""

from __future__ import annotations

import hashlib
import json
import os
import re
import struct
from dataclasses import dataclass
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Any

from eltdx.protocol.unit import ID_TO_MARKET, MARKET_TO_ID


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
        self._client = client
        selected_data_dir: str | os.PathLike[str] = data_dir or os.environ.get(
            "ELTDX_BOARD_DATA_DIR", str(Path.cwd() / "downloads")
        )
        self._data_dir = Path(selected_data_dir).expanduser()
        self._definitions_dir = Path(definitions_dir).expanduser() if definitions_dir else None
        self._prepared_date: date | None = None
        self._boards: tuple[dict[str, Any], ...] | None = None
        self._members: dict[str, dict[str, Any]] = {}
        self._security: dict[tuple[int, str], Any] | None = None

    def clear_cache(self) -> None:
        self._prepared_date = None
        self._boards = None
        self._members.clear()
        self._security = None

    def board_quotes(self, *, category: str = "概念", refresh: bool = False) -> BoardQuoteTable:
        if category not in BOARD_CATEGORIES:
            raise ValueError(f"unsupported board category: {category!r}; choose from {BOARD_CATEGORIES}")
        prepared = self._prepare(refresh=refresh)
        selected = tuple(item for item in prepared["boards"] if _matches_category(item, category))
        board_codes = [item["full_code"] for item in selected]
        quotes = self._snapshot_batches(board_codes)
        by_code = {str(getattr(item, "full_code", "")): item for item in quotes}
        rows = tuple(self._quote_row(board, by_code.get(board["full_code"])) for board in selected)
        return BoardQuoteTable(rows=rows, prepared_date=prepared["date"])

    def board_member_quotes(self, board_code: str, *, refresh: bool = False) -> BoardMemberQuoteTable:
        if not isinstance(board_code, str) or not board_code.strip():
            raise ValueError("board_code is required")
        prepared = self._prepare(refresh=refresh)
        key = board_code.strip().lower()
        board = next((item for item in prepared["boards"] if item["board_code"].lower() == key or item["full_code"].lower() == key), None)
        if board is None:
            raise ValueError(f"unknown board code: {board_code!r}")
        detail = self._members_for(board)
        codes = [self._full_security_code(item["market"], item["code"]) for item in detail["display_members"]]
        quotes = self._snapshot_batches(codes)
        by_code = {str(getattr(item, "full_code", "")): item for item in quotes}
        rows = tuple(
            self._quote_row({"board_code": board["board_code"], "board_name": board["board_name"], "full_code": code}, by_code.get(code))
            for code in codes
        )
        return BoardMemberQuoteTable(
            board_code=board["board_code"], board_name=board["board_name"],
            raw_members=tuple(detail["raw_members"]), display_members=tuple(detail["display_members"]),
            excluded_members=tuple(detail["excluded_members"]), rows=rows,
            prepared_date=prepared["date"],
        )

    def _prepare(self, *, refresh: bool) -> dict[str, Any]:
        today = self._market_date()
        if not refresh and self._prepared_date == today and self._boards is not None and self._security is not None:
            return {"date": today, "boards": self._boards}
        self._data_dir.mkdir(parents=True, exist_ok=True)
        metadata_path = self._data_dir / ".eltdx_board_cache.json"
        metadata = _read_json(metadata_path)
        if not refresh and metadata.get("last_success_date") == today.isoformat() and self._cache_files_ready():
            boards = self._parse_boards()
            security = self._load_security_file()
            if boards and security is not None:
                self._boards, self._security, self._prepared_date = tuple(boards), security, today
                return {"date": today, "boards": self._boards}

        # Existing files are valid only for the date recorded in metadata.
        # When that date changes, fetch a fresh 0x06b9 snapshot once.
        self._download_missing_or_refresh(
            refresh=refresh or metadata.get("last_success_date") != today.isoformat()
        )
        boards = self._parse_boards()
        if not boards:
            raise RuntimeError("no board definitions found; configure definitions_dir or ELTDX_BOARD_DATA_DIR")
        # The first call of a new day refreshes the 0x044d universe.  A local
        # JSON file is only a fallback for offline fixtures or a failed server.
        security = self._load_security_from_044d()
        if not security:
            security = self._load_security_file() or {}
        else:
            self._write_security_file(security)
        self._boards, self._security, self._prepared_date = tuple(boards), security, today
        metadata_path.write_text(json.dumps({
            "last_success_date": today.isoformat(),
            "downloaded_at": datetime.now(timezone.utc).isoformat(),
            "sha256": {name: _sha256(self._data_dir / name) for name in BOARD_FILES if (self._data_dir / name).exists()},
        }, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        return {"date": today, "boards": self._boards}

    def _download_missing_or_refresh(self, *, refresh: bool) -> None:
        for name in BOARD_FILES:
            target = self._data_dir / name
            if target.exists() and not refresh:
                continue
            try:
                payload = self._client.resources.download_file(name)
            except Exception:
                continue
            if isinstance(payload, (bytes, bytearray)) and payload:
                target.write_bytes(bytes(payload))

    def _cache_files_ready(self) -> bool:
        return any((self._data_dir / name).exists() for name in BOARD_FILES) and bool(self._definition_paths())

    def _definition_paths(self) -> list[Path]:
        candidates = []
        if self._definitions_dir:
            candidates.append(self._definitions_dir)
        candidates.append(self._data_dir)
        candidates.append(Path.cwd() / "downloads")
        candidates.append(Path(r"C:\APP\tdx\T0002\hq_cache"))
        for root in candidates:
            if all((root / name).exists() for name in DEFINITION_FILES):
                return [root / name for name in DEFINITION_FILES]
        return []

    def _parse_boards(self) -> list[dict[str, Any]]:
        definitions: dict[str, list[str]] = {}
        paths = self._definition_paths()
        for path in paths:
            for line in _read_text(path).splitlines():
                fields = line.split("|")
                if len(fields) >= 6 and re.fullmatch(r"\d{6}", fields[1].strip()):
                    definitions.setdefault(fields[1].strip(), fields)
        # Preserve the concept/style classification when only infoharbor is available.
        for code, name, category in _infoharbor_headers(self._data_dir / "infoharbor_block.dat"):
            definitions.setdefault(code, [name, code, str(category), "1", "0", ""])
        result = []
        for code, fields in definitions.items():
            category = int(fields[2]) if fields[2].isdigit() else 4
            board_name = fields[0].strip() or code
            market = _board_market(code, fields[3] if len(fields) > 3 else None)
            result.append({"board_code": code, "board_name": board_name, "category": category,
                           "membership_key": fields[5].strip() if len(fields) > 5 else "",
                           "full_code": market + code, "market_id": MARKET_TO_ID[market]})
        return result

    def _members_for(self, board: dict[str, Any]) -> dict[str, Any]:
        code = board["board_code"]
        if code in self._members:
            return self._members[code]
        category, key = board["category"], board["membership_key"]
        members: list[tuple[int, str]] = []
        if category in (4, 5):
            active = False
            expected = None
            path = self._data_dir / "infoharbor_block.dat"
            for line in _read_text(path).splitlines():
                if line.startswith("#"):
                    parts = line.split(",")
                    active = len(parts) > 2 and parts[2].strip() == code
                    expected = int(parts[1]) if active and parts[1].isdigit() else None
                elif active:
                    members.extend((int(m), c) for m, c in re.findall(r"([012])#(\d{6})", line))
            if expected is not None and len(set(members)) != expected:
                raise ValueError(f"incomplete board membership: {code}")
        elif category in (2, 12):
            column = 2 if category == 2 else 5
            for line in _read_text(self._data_dir / "tdxhy.cfg").splitlines():
                fields = line.split("|")
                if len(fields) > column and fields[column].startswith(key):
                    members.append((int(fields[0]), fields[1]))
        elif category == 3:
            members = list(_read_dbf_members(self._data_dir / "base.dbf", key))
        else:
            raise ValueError(f"unsupported board category: {category}")
        unique = []
        seen: set[tuple[int, str]] = set()
        for item in members:
            if item not in seen:
                seen.add(item)
                unique.append(item)
        raw = [{"market": market, "code": code} for market, code in unique]
        security = self._security or {}
        display = [{"market": market, "code": code} for market, code in unique if (market, code) in security]
        excluded = [{"market": market, "code": code, "reason": "absent_from_current_security_list"}
                    for market, code in unique if (market, code) not in security]
        result = {"raw_members": raw, "display_members": display, "excluded_members": excluded}
        self._members[code] = result
        return result

    def _load_security_from_044d(self) -> dict[tuple[int, str], Any]:
        result: dict[tuple[int, str], Any] = {}
        for market in ("sz", "sh", "bj"):
            try:
                rows = self._client.codes.all(market)
            except Exception:
                rows = ()
            for row in rows or ():
                market_id = getattr(row, "market_id", MARKET_TO_ID[market])
                code = str(getattr(row, "code", ""))
                if code:
                    result[(int(market_id), code)] = row
        return result

    def _load_security_file(self) -> dict[tuple[int, str], Any] | None:
        path = self._data_dir / "security_list.json"
        if not path.exists():
            return None
        try:
            rows = json.loads(path.read_text(encoding="utf-8"))
            return {(int(item["market"]), str(item["code"])): item for item in rows}
        except (OSError, ValueError, KeyError, TypeError):
            return None

    def _write_security_file(self, security: dict[tuple[int, str], Any]) -> None:
        rows: list[dict[str, Any]] = []
        for (market, code), item in sorted(security.items()):
            rows.append({"market": market, "code": code, "name": getattr(item, "name", None)})
        (self._data_dir / "security_list.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    def _snapshot_batches(self, codes: list[str]) -> list[Any]:
        rows: list[Any] = []
        for start in range(0, len(codes), BOARD_BATCH_SIZE):
            page = self._client.quotes.get_snapshots(codes[start:start + BOARD_BATCH_SIZE])
            if isinstance(page, (list, tuple)):
                rows.extend(page)
        return rows

    @staticmethod
    def _quote_row(board: dict[str, Any], quote: Any | None) -> BoardQuoteRow:
        full_code = str(board["full_code"])
        last = getattr(quote, "last_price", None)
        pre = getattr(quote, "pre_close_price", None)
        change = last - pre if last is not None and pre is not None else None
        pct = change / pre * 100 if change is not None and pre else None
        return BoardQuoteRow(board_code=board["board_code"], board_name=board["board_name"], full_code=full_code,
                             exchange=full_code[:2], market_id=MARKET_TO_ID.get(full_code[:2], board.get("market_id", 0)),
                             last_price=last, pre_close_price=pre, open_price=getattr(quote, "open_price", None),
                             high_price=getattr(quote, "high_price", None), low_price=getattr(quote, "low_price", None),
                             change=change, change_pct=pct, amount=getattr(quote, "amount", None),
                             volume_hand=getattr(quote, "total_hand", None), quote=quote)

    def _market_date(self) -> date:
        try:
            handshake = self._client.session.handshake()
            values: list[date] = [
                value
                for name in ("server_date_1", "server_date_2")
                if isinstance(value := getattr(handshake, name, None), date)
            ]
            if values:
                return max(values)
        except Exception:
            pass
        return date.today()

    @staticmethod
    def _full_security_code(market: int, code: str) -> str:
        return ID_TO_MARKET[int(market)] + str(code).zfill(6)


def _read_text(path: Path) -> str:
    for encoding in ("gb18030", "gbk", "utf-8"):
        try:
            return path.read_text(encoding=encoding)
        except (UnicodeDecodeError, OSError):
            continue
    return ""


def _read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else {}
    except (OSError, ValueError):
        return {}


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _board_market(code: str, declared: str | None) -> str:
    if code.startswith("399"):
        return "sz"
    if code.startswith("880") or code.startswith("881"):
        return "sh"
    if declared in {"0", "1", "2"}:
        return ID_TO_MARKET[int(declared)]
    return "sh"


def _matches_category(board: dict[str, Any], category: str) -> bool:
    value = int(board["category"])
    if category == "全部":
        return True
    if category == "概念":
        return value == 4
    if category == "风格":
        return value == 5
    if category == "地区":
        return value == 3
    if category == "880行业":
        return value == 2
    if category == "二级行业":
        return value == 12 and re.fullmatch(r"X\d{4}", board["membership_key"]) is not None
    if category == "一级行业":
        return value == 12 and re.fullmatch(r"X\d{2}", board["membership_key"]) is not None
    if category == "三级行业":
        return value == 12 and re.fullmatch(r"X\d{6}", board["membership_key"]) is not None
    return False


def _infoharbor_headers(path: Path) -> list[tuple[str, str, int]]:
    result = []
    for line in _read_text(path).splitlines():
        if not line.startswith("#"):
            continue
        fields = line.split(",")
        if len(fields) >= 3 and re.fullmatch(r"\d{6}", fields[2].strip()):
            name = fields[0][1:].strip()
            if name.startswith("FG_"):
                result.append((fields[2].strip(), name[3:], 5))
            elif name.startswith("GN_"):
                result.append((fields[2].strip(), name[3:], 4))
    return result


def _read_dbf_members(path: Path, region_key: str) -> set[tuple[int, str]]:
    data = path.read_bytes()
    count, header_size, record_size = struct.unpack_from("<IHH", data, 4)
    fields: dict[str, tuple[int, int]] = {}
    offset = 1
    for pos in range(32, header_size - 1, 32):
        if data[pos] == 13:
            break
        name = data[pos:pos + 11].split(b"\0")[0].decode("ascii")
        size = data[pos + 16]
        fields[name] = offset, size
        offset += size
    result = set()
    for index in range(count):
        record = data[header_size + index * record_size:header_size + (index + 1) * record_size]
        if not record or record[0] == 42:
            continue
        values = {name: record[start:start + size].decode("ascii").strip() for name, (start, size) in fields.items() if name in {"SC", "GPDM", "DY"}}
        try:
            same_region = int(values.get("DY", "-1")) == int(region_key)
        except ValueError:
            same_region = False
        if same_region and re.fullmatch(r"\d{6}", values.get("GPDM", "")):
            result.add((int(values.get("SC", "0")), values["GPDM"]))
    return result
