from __future__ import annotations

import pytest
from dataclasses import replace
from datetime import timedelta
from pathlib import Path
from threading import Barrier, Lock

from eltdx import TdxClient
from eltdx.api import DelistedBarApi
from eltdx.exceptions import ProtocolError
from eltdx.protocol.commands import build_command_frame, command_code, parse_command_response
from eltdx.protocol.frame import ResponseFrame
from eltdx.protocol import decode_response


def test_delisted_bars_uses_the_verified_command_and_fixed_daily_contract() -> None:
    client = TdxClient.in_memory()

    result = client.delisted_bars.get("sz000038", count=20)

    assert command_code("delisted_bars") == 0x052B
    assert result["command"] == "0x052b"
    assert result["payload"] == {
        "code": "sz000038",
        "start": 0,
        "count": 20,
        "include_raw": False,
    }


@pytest.mark.parametrize("period", ["week", "month", "5m"])
def test_delisted_bars_rejects_unverified_periods(period: str) -> None:
    client = TdxClient.in_memory()

    with pytest.raises(ValueError, match="period"):
        client.delisted_bars.get("sz000038", period=period)


@pytest.mark.parametrize("adjust", ["qfq", "hfq", "fixed_qfq", "fixed_hfq"])
def test_delisted_bars_rejects_unverified_adjustments(adjust: str) -> None:
    client = TdxClient.in_memory()

    with pytest.raises(ValueError, match="adjust"):
        client.delisted_bars.get("sz000038", adjust=adjust)


def test_delisted_bars_request_matches_the_12_byte_wire_body() -> None:
    frame = build_command_frame(
        0x052B,
        {"code": "sz000038", "start": 800, "count": 20},
        7,
    )

    assert frame.msg_type == 0x052B
    assert frame.data == bytes.fromhex("000030303030333820031400")


@pytest.mark.parametrize("code", ["T000038", "t000038"])
def test_delisted_prefix_is_removed_only_for_delisted_requests(code: str) -> None:
    frame = build_command_frame(0x052B, {"code": code, "count": 20}, 7)
    assert frame.data == bytes.fromhex("000030303030333800001400")
    with pytest.raises(ValueError):
        build_command_frame(0x052D, {"code": code}, 7)


@pytest.mark.parametrize("adjust", [0, False, []])
def test_delisted_bars_rejects_invalid_adjustment_types(adjust) -> None:
    with pytest.raises(ValueError, match="adjust"):
        TdxClient.in_memory().delisted_bars.get("sz000038", adjust=adjust)


def _sample_page():
    # First record of the independently captured 2023-04-24..2023-07-11 page.
    payload = bytes.fromhex("010018b13401b833b40288030084e49a04dbb8674b00000100")
    response = ResponseFrame(0, 7, 0x052B, len(payload), len(payload), payload, b"")
    return parse_command_response(0x052B, response, {"code": "sz000038", "count": 1})


def test_delisted_paging_continues_short_pages_and_sorts_history() -> None:
    page = _sample_page()
    earlier_bar = replace(page.bars[0], time=page.bars[0].time - timedelta(days=1))
    pages = {0: page, 1: replace(page, bars=(earlier_bar,)), 2: replace(page, bars=())}

    class RecordedTransport:
        def execute(self, command, payload):
            assert command == 0x052B
            return pages[payload["start"]]

    history = DelistedBarApi(RecordedTransport()).get("sz000038", all_pages=True, page_size=800)
    assert history.bars == (earlier_bar, page.bars[0])
    assert history.count == history.request_count == 2


def test_delisted_paging_raises_when_limit_prevents_reaching_empty_page() -> None:
    class RecordedTransport:
        def execute(self, command, payload):
            return _sample_page()

    with pytest.raises(RuntimeError, match="max_pages"):
        DelistedBarApi(RecordedTransport()).get("sz000038", all_pages=True, max_pages=1)


@pytest.mark.parametrize("include_raw", [False, True])
def test_delisted_capture_decodes_prices_volume_and_raw_fields(include_raw: bool) -> None:
    path = Path(__file__).parent / "fixtures/7709/delisted_bars/normal/response.bin"
    response = decode_response(path.read_bytes())
    series = parse_command_response(
        0x052B, response, {"code": "T000038", "count": 20, "include_raw": include_raw}
    )
    assert series.full_code == "sz000038"
    assert series.period_name == "day"
    assert series.adjust_mode == "none"
    assert series.count == 20
    first, last = series.bars[0], series.bars[-1]
    assert first.time.isoformat() == "2023-04-24T00:00:00+08:00"
    assert (first.open, first.close, first.high, first.low) == (3.32, 3.5, 3.52, 3.32)
    assert last.time.isoformat() == "2023-07-11T00:00:00+08:00"
    assert (last.open, last.close, last.high, last.low) == (0.54, 0.52, 0.55, 0.51)
    assert last.volume_raw == last.volume_wire_value == 20940451
    assert last.volume_lots == 209404.51
    assert last.amount == 10931139
    assert first.last_close_price_milli is None
    assert series.bars[1].last_close_price_milli == 3500
    assert last.record_hex == (
        "37b234014a540a5ea39afc13c3cb264b00000100" if include_raw else ""
    )
    assert series.raw_payload == (response.data if include_raw else b"")


@pytest.mark.parametrize("payload", [
    b"", b"\x01", b"\x01\x00",  # Missing count / records.
    bytes.fromhex("010018b13401b833b40288030084e49a04dbb8674b000001"),  # Truncated suffix.
    bytes.fromhex("010000000000b833b40288030084e49a04dbb8674b00000100"),  # Invalid date.
    bytes.fromhex("010018b13401b833b402000084e49a04dbb8674b00000100"),  # High < close.
    bytes.fromhex("010018b13401b833b402880300410000000000000100"),  # Negative volume.
    b"\x00\x00\x01",  # Trailing bytes.
])
def test_delisted_parser_rejects_malformed_records(payload: bytes) -> None:
    response = ResponseFrame(0, 7, 0x052B, len(payload), len(payload), payload, b"")
    with pytest.raises(ProtocolError):
        parse_command_response(0x052B, response, {"code": "sz000038", "count": 1})


@pytest.mark.parametrize("count", [0, 801, -1, True])
def test_delisted_bars_rejects_out_of_range_counts(count) -> None:
    with pytest.raises(ValueError):
        TdxClient.in_memory().delisted_bars.get("sz000038", count=count)


def test_delisted_batch_normalizes_deduplicates_and_preserves_code_order() -> None:
    path = Path(__file__).parent / "fixtures/7709/delisted_bars/normal/response.bin"
    response = decode_response(path.read_bytes())
    requested_codes = []

    class RecordedTransport:
        def execute(self, command, payload):
            assert command == 0x052B
            requested_codes.append(payload["code"])
            return parse_command_response(command, response, payload)

    result = DelistedBarApi(RecordedTransport()).get(
        ["T000038", "sz000038", "000038", "t600001"], count=20, include_raw=True
    )
    assert list(result) == ["sz000038", "sh600001"]
    assert requested_codes == ["sz000038", "sh600001"]
    for code, series in result.items():
        assert series.full_code == code
        assert series.count == 20
        assert series.raw_payload == response.data
        assert series.bars[-1].close == 0.52


def test_delisted_batch_paginates_each_stock_independently() -> None:
    page = _sample_page()
    earlier = replace(page.bars[0], time=page.bars[0].time - timedelta(days=1))
    stock_pages = {
        "sz000038": {0: page, 1: replace(page, bars=(earlier,)), 2: replace(page, bars=())},
        "sz002087": {0: page, 1: replace(page, bars=())},
    }

    class RecordedTransport:
        pool_size = 2

        def execute(self, command, payload):
            assert payload["count"] == 800
            return replace(stock_pages[payload["code"]][payload["start"]],
                           code=payload["code"][2:])

    result = DelistedBarApi(RecordedTransport()).get(
        ("T000038", "002087"), all_pages=True
    )
    assert result["sz000038"].bars == (earlier, page.bars[0])
    assert result["sz000038"].count == result["sz000038"].request_count == 2
    assert result["sz002087"].bars == page.bars
    assert result["sz002087"].count == result["sz002087"].request_count == 1


@pytest.mark.parametrize("pool_size,batch_size,expected", [
    (2, None, 2), (2, 9, 2), (3, 2, 2), (3, 1, 1), (None, None, 1),
])
def test_delisted_batch_bounds_concurrency(pool_size, batch_size, expected) -> None:
    barrier = Barrier(2)
    lock = Lock()
    page = _sample_page()

    class RecordedTransport:
        active = peak = 0

        def execute(self, command, payload):
            with lock:
                self.active += 1
                self.peak = max(self.peak, self.active)
            try:
                if expected == 2:
                    barrier.wait(timeout=5)
                return replace(page, code=payload["code"][2:])
            finally:
                with lock:
                    self.active -= 1

    transport = RecordedTransport()
    transport.pool_size = pool_size
    result = DelistedBarApi(transport).get(
        ["000038", "002087"], batch_size=batch_size
    )
    assert list(result) == ["sz000038", "sz002087"]
    assert transport.peak == expected


@pytest.mark.parametrize("batch_size", [0, -1, True, 1.5, "2"])
@pytest.mark.parametrize("code", ["T000038", ["T000038"]])
def test_delisted_batch_rejects_invalid_concurrency(code, batch_size) -> None:
    with pytest.raises(ValueError, match="batch_size"):
        TdxClient.in_memory().delisted_bars.get(code, batch_size=batch_size)


@pytest.mark.parametrize("codes", [[], ["T000038", "invalid"], ["T000038", None]])
def test_delisted_batch_validates_all_codes_before_querying(codes) -> None:
    class NoRequestsTransport:
        def execute(self, command, payload):
            pytest.fail("invalid batch must be rejected before querying")

    with pytest.raises((ValueError, ProtocolError)):
        DelistedBarApi(NoRequestsTransport()).get(codes)


def test_delisted_batch_propagates_query_failure() -> None:
    class FailedTransport:
        def execute(self, command, payload):
            if payload["code"] == "sz002087":
                raise ProtocolError("server failed for sz002087")
            return _sample_page()

    with pytest.raises(ProtocolError, match="sz002087"):
        DelistedBarApi(FailedTransport()).get(["000038", "002087"])
