from __future__ import annotations

from unittest.mock import Mock

from eltdx.client import TdxClient


def test_client_clamps_batch_size() -> None:
    client = TdxClient(batch_size=999)
    assert client._batch_size == 80


class _ImmediateFuture:
    def __init__(self, value) -> None:
        self._value = value

    def result(self):
        return self._value


class _ImmediateExecutor:
    def submit(self, fn, *args, **kwargs):
        return _ImmediateFuture(fn(*args, **kwargs))


def test_get_quote_auto_batches_large_requests_across_two_connections() -> None:
    client = TdxClient(pool_size=2, batch_size=80)
    client.connect = Mock()
    client._executor = _ImmediateExecutor()

    connection_a = Mock()
    connection_a.request_quotes.side_effect = lambda codes: [f"a:{len(codes)}"]
    connection_b = Mock()
    connection_b.request_quotes.side_effect = lambda codes: [f"b:{len(codes)}"]
    client._connections = [connection_a, connection_b]

    codes = [f"sz{index:06d}" for index in range(6000)]
    parsed = client.get_quote(codes)

    assert len(parsed) == 75
    assert connection_a.request_quotes.call_count == 38
    assert connection_b.request_quotes.call_count == 37
    assert connection_a.request_quotes.call_args_list[0].args[0] == codes[:80]
    assert connection_b.request_quotes.call_args_list[0].args[0] == codes[80:160]
