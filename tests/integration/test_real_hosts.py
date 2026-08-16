from __future__ import annotations

import os
from collections.abc import Callable
from typing import Any

import pytest

from eltdx import TdxClient
from eltdx.exceptions import ConnectionClosedError, ResponseTimeoutError
from eltdx.protocol.constants import TYPE_FILE_CONTENT


DEFAULT_CODES = ("sz000001", "sh600000", "bj920001")
DEFAULT_ETF = "sh510300"
DEFAULT_INDEX = "sh000001"
DEFAULT_HISTORY_DATE = "2026-08-14"
DEFAULT_RESOURCE_PATH = "T0002/hq_cache.dat"


def _hosts() -> list[str] | None:
    raw = os.environ.get("ELTDX_REAL_HOSTS")
    if raw is None:
        return None
    hosts = [value.strip() for value in raw.split(",") if value.strip()]
    if not hosts:
        raise ValueError("ELTDX_REAL_HOSTS does not contain a host")
    return hosts


def _client() -> TdxClient:
    hosts = _hosts()
    if hosts is None:
        return TdxClient(timeout=8, pool_size=4, heartbeat_interval=2)
    return TdxClient.from_hosts(
        hosts,
        timeout=8,
        pool_size=4,
        heartbeat_interval=2,
    )


def _size(value: Any) -> int | None:
    if isinstance(value, (list, tuple, bytes, str)):
        return len(value)
    for field in ("records", "bars", "points", "ticks"):
        items = getattr(value, field, None)
        if isinstance(items, (list, tuple)):
            return len(items)
    count = getattr(value, "count", None)
    return count if isinstance(count, int) else None


def test_all_native_commands_against_real_hosts() -> None:
    codes = tuple(os.environ.get("ELTDX_REAL_CODES", ",".join(DEFAULT_CODES)).split(","))
    codes = tuple(code.strip() for code in codes if code.strip())
    if len(codes) < 3:
        raise ValueError("ELTDX_REAL_CODES must contain Shenzhen, Shanghai, and Beijing codes")
    stock = codes[0]
    etf = os.environ.get("ELTDX_REAL_ETF", DEFAULT_ETF)
    index = os.environ.get("ELTDX_REAL_INDEX", DEFAULT_INDEX)
    history_date = os.environ.get("ELTDX_REAL_HISTORY_DATE", DEFAULT_HISTORY_DATE)
    resource_path = os.environ.get("ELTDX_REAL_RESOURCE_PATH", DEFAULT_RESOURCE_PATH)
    external_failures: list[str] = []
    client_failures: list[str] = []
    results: dict[str, tuple[str, int | None]] = {}

    with _client() as client:
        calls: list[tuple[str, Callable[[], Any]]] = [
            ("heartbeat", client.session.heartbeat),
            ("handshake", client.session.handshake),
            ("security_count", lambda: client.codes.count("sz")),
            ("security_list", lambda: client.codes.list("bj", limit=5)),
            ("special_limits", lambda: client.limits.special(start_index=0)),
            ("intraday_aux", lambda: client.minutes.aux(stock)),
            ("klines", lambda: client.bars.get(stock, count=20)),
            ("today_intraday", lambda: client.minutes.today(stock)),
            ("legacy_quotes", lambda: client.quotes.legacy(codes)),
            ("refresh_stream", lambda: client.quotes.refresh(codes, cursors={})),
            ("category_quotes", lambda: client.quotes.list_by_category(6, count=5)),
            ("snapshots", lambda: client.quotes.get_snapshots(codes)),
            ("auction_series", lambda: client.auctions.series(stock)),
            ("file_content", lambda: client.resources.read(resource_path, size=64)),
            (
                "historical_intraday",
                lambda: client.minutes.history(stock, history_date),
            ),
            ("today_ticks", lambda: client.trades.today(stock, count=20)),
            (
                "historical_ticks",
                lambda: client.trades.history(stock, history_date, count=20),
            ),
            ("sparkline", lambda: client.minutes.sparkline(stock)),
            ("recent_intraday", lambda: client.minutes.recent(stock, history_date)),
            ("capital_changes", lambda: client.corporate.capital_changes(stock)),
            ("finance_batch", lambda: client.corporate.finance_batch(codes)),
        ]
        for name, call in calls:
            try:
                value = call()
                results[name] = (type(value).__name__, _size(value))
            except (ConnectionClosedError, ResponseTimeoutError) as error:
                external_failures.append(f"{name}: {type(error).__name__}: {error}")
            except BaseException as error:
                client_failures.append(f"{name}: {type(error).__name__}: {error}")

        for name, code, kind in (
            ("klines_etf", etf, "stock"),
            ("klines_index", index, "index"),
        ):
            try:
                value = client.bars.get(code, count=20, kind=kind)
                results[name] = (type(value).__name__, _size(value))
            except (ConnectionClosedError, ResponseTimeoutError) as error:
                external_failures.append(f"{name}: {type(error).__name__}: {error}")
            except BaseException as error:
                client_failures.append(f"{name}: {type(error).__name__}: {error}")

        try:
            with client.transport.pin() as pinned:
                value = pinned.execute(
                    TYPE_FILE_CONTENT,
                    {"path": resource_path, "offset": 0, "size": 64},
                )
                results["file_content_pin"] = (type(value).__name__, _size(value))
        except (ConnectionClosedError, ResponseTimeoutError) as error:
            external_failures.append(f"file_content_pin: {type(error).__name__}: {error}")
        except BaseException as error:
            client_failures.append(f"file_content_pin: {type(error).__name__}: {error}")

        try:
            push = client.quotes.poll_push(timeout=0.2, parse=False)
            results["push_poll"] = (type(push).__name__, _size(push))
        except (ConnectionClosedError, ResponseTimeoutError) as error:
            external_failures.append(f"push_poll: {type(error).__name__}: {error}")
        except BaseException as error:
            client_failures.append(f"push_poll: {type(error).__name__}: {error}")

    if client_failures:
        pytest.fail("real-host client errors:\n" + "\n".join(client_failures))
    if external_failures:
        pytest.fail("real-host external service failures:\n" + "\n".join(external_failures))
    expected_commands = {name for name, _ in calls}
    assert expected_commands <= results.keys()
    assert {"klines_etf", "klines_index", "file_content_pin", "push_poll"} <= results.keys()
