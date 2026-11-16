from __future__ import annotations

from types import SimpleNamespace

import pytest

from eltdx.api.codes import CodeApi
from eltdx.api.quotes import QuoteApi
from eltdx.client import TdxClient
from eltdx.exceptions import MissingCodesError, MissingCodesWarning
from eltdx.models.quote import QuoteSnapshot
from eltdx.transport.socket import SocketTransport


def _snapshot(code: str = "600000") -> QuoteSnapshot:
    return QuoteSnapshot(
        "sh", 1, code, 0, 1.0, 1.0, 1.0, 1.0, 1.0, 0, 0, 0, 0,
        0.0, 0, 0, 0, 0, 0, 0.0, (), (), b"",
    )


def test_native_snapshot_bridge_projects_missing_codes_before_return() -> None:
    class NativeTransport:
        def sdk_snapshots(self, codes):
            assert codes == ["sh600000", "sz000002"]
            return [_snapshot(), None], ("sz000002",)

        def execute(self, *_args, **_kwargs):
            pytest.fail("native snapshots must not call the Python transport algorithm")

    quotes = QuoteApi(NativeTransport(), native_services=True)

    with pytest.raises(MissingCodesError) as caught:
        quotes.get_snapshots(["sh600000", "sz000002"])

    assert caught.value.missing_codes == ("sz000002",)


def test_native_snapshot_bridge_skip_mode_keeps_occurrence_order() -> None:
    class NativeTransport:
        def sdk_snapshots(self, _codes):
            return [_snapshot("600000"), None, _snapshot("600000")], ("sh600001",)

        def execute(self, *_args, **_kwargs):
            pytest.fail("native snapshots must not call the Python transport algorithm")

    quotes = QuoteApi(NativeTransport(), native_services=True)

    with pytest.warns(MissingCodesWarning, match="sh600001"):
        rows = quotes.get_snapshots(
            ["sh600000", "sh600001", "sh600000"], on_missing="skip"
        )

    assert [row.full_code for row in rows] == ["sh600000", "sh600000"]


def test_native_snapshot_bridge_keeps_price_metadata_resolution_order() -> None:
    codes = ["sh600000"]
    calls = []

    def resolve(command, result, requested):
        calls.append((command, result, requested))
        return result

    class NativeTransport:
        def sdk_snapshots(self, _codes):
            return [_snapshot()], ()

    quotes = QuoteApi(
        NativeTransport(), price_resolver=resolve, native_services=True
    )

    rows = quotes.get_snapshots(codes)

    assert rows == [_snapshot()]
    assert calls == [
        ("snapshots", None, codes),
        ("snapshots", rows, codes),
    ]


def test_native_snapshot_bridge_projects_partial_result_before_missing_error() -> None:
    codes = ["sz000001"]
    calls = []

    def resolve(command, result, requested):
        calls.append((command, result, requested))
        return result

    class NativeTransport:
        def sdk_snapshots(self, _codes):
            return [None], ("sz000001",)

    quotes = QuoteApi(
        NativeTransport(), price_resolver=resolve, native_services=True
    )

    with pytest.raises(MissingCodesError):
        quotes.get_snapshots(codes)

    assert calls == [
        ("snapshots", None, codes),
        ("snapshots", [], codes),
    ]


def test_custom_transport_keeps_the_existing_snapshot_recovery_path() -> None:
    row = _snapshot()

    class CustomTransport:
        def __init__(self):
            self.calls = []

        def execute(self, command, payload):
            self.calls.append((command, payload))
            return [row]

    transport = CustomTransport()
    quotes = QuoteApi(transport)

    assert quotes.get_snapshots("sh600000") == [row]
    assert len(transport.calls) == 1


def test_socket_transport_subclass_uses_its_custom_execute_path() -> None:
    class CustomSocketTransport(SocketTransport):
        def execute(self, command, payload=None):
            return (command, payload)

    client = TdxClient(transport=CustomSocketTransport(hosts=["127.0.0.1:7709"]))

    assert not client.quotes._native_services
    assert not client.codes._native_services


def test_native_code_scan_uses_rust_pages_and_keeps_metadata_sink() -> None:
    rows = [SimpleNamespace(full_code="sh600000", decimal=2)]

    class NativeTransport:
        def sdk_security_pages(self, market, page_size):
            assert (market, page_size) == ("sh", 800)
            return [rows, []]

        def execute(self, *_args, **_kwargs):
            pytest.fail("native directory scans must use the Rust paginator")

    seen = []

    def sink(page):
        seen.append(page)

    codes = CodeApi(NativeTransport(), metadata_sink=sink, native_services=True)

    assert codes.all("sh", page_size=800) == rows
    assert seen == [rows, []]
