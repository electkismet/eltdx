from __future__ import annotations

from unittest.mock import Mock

from eltdx.client import TdxClient


def test_client_context_manager_connects_and_closes() -> None:
    client = TdxClient()
    client.connect = Mock()
    client.close = Mock()

    with client as current:
        assert current is client

    client.connect.assert_called_once_with()
    client.close.assert_called_once_with()
