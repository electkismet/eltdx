import queue
from dataclasses import dataclass

import pytest

from eltdx import TdxClient
from eltdx.http_server import create_app


def _fastapi_test_client():
    pytest.importorskip("fastapi")
    from starlette.testclient import TestClient

    return TestClient


def test_http_gateway_is_optional_and_exposes_health_and_methods() -> None:
    TestClient = _fastapi_test_client()
    with TestClient(create_app(client=TdxClient.in_memory())) as client:
        health = client.get("/health")
        assert health.status_code == 200
        assert health.json()["transports"] == ["http", "websocket"]

        methods = client.get("/methods")
        assert methods.status_code == 200
        assert "bars.get" in methods.json()["methods"]
        assert methods.json()["websocket_only"] == [
            "quotes.subscribe",
            "quotes.unsubscribe",
        ]


def test_http_gateway_maps_rpc_errors_to_json() -> None:
    TestClient = _fastapi_test_client()
    with TestClient(create_app(client=TdxClient.in_memory())) as client:
        response = client.post("/rpc", json={"id": 7, "method": "does.not.exist"})
        assert response.status_code == 404
        payload = response.json()
        assert payload["ok"] is False
        assert payload["id"] == 7
        assert payload["error"]["type"] == "GatewayMethodError"


def test_http_gateway_supports_websocket_request_response() -> None:
    TestClient = _fastapi_test_client()
    with TestClient(create_app(client=TdxClient.in_memory())) as client:
        with client.websocket_connect("/ws") as websocket:
            websocket.send_json({"id": "x", "method": "ping", "params": {}})
            response = websocket.receive_json()
            assert response["id"] == "x"
            assert response["ok"] is True


def test_http_gateway_rejects_malformed_rpc() -> None:
    TestClient = _fastapi_test_client()
    with TestClient(create_app(client=TdxClient.in_memory())) as client:
        response = client.post("/rpc", json={"id": 1})
        assert response.status_code == 400
        assert response.json()["ok"] is False


def test_websocket_quote_subscription_sends_initial_and_native_update() -> None:
    @dataclass(frozen=True)
    class Record:
        full_code: str
        last_price: float

    @dataclass(frozen=True)
    class Page:
        records: tuple[Record, ...]

    class Quotes:
        def __init__(self) -> None:
            self.pushes: queue.Queue[Page] = queue.Queue()

        def get_depth(self, codes):
            return {"requested_codes": codes, "records": []}

        def poll_push(self, *, timeout=0.0, parse=False):
            assert parse is True
            try:
                return self.pushes.get(timeout=timeout)
            except queue.Empty:
                return None

    class Client:
        def __init__(self) -> None:
            self.quotes = Quotes()

    runtime_client = Client()
    TestClient = _fastapi_test_client()
    with TestClient(create_app(client=runtime_client)) as client:
        with client.websocket_connect("/ws") as websocket:
            websocket.send_json(
                {
                    "id": 1,
                    "method": "quotes.subscribe",
                    "params": {"codes": ["sz000001"]},
                }
            )
            response = websocket.receive_json()
            assert response["ok"] is True
            assert response["result"]["codes"] == ["sz000001"]
            subscription_id = response["result"]["subscription_id"]

            runtime_client.quotes.pushes.put(
                Page((Record("sz000001", 12.34), Record("sh600000", 9.87)))
            )
            event = websocket.receive_json()
            assert event == {
                "event": "quote",
                "subscription_id": subscription_id,
                "data": {"full_code": "sz000001", "last_price": 12.34},
            }

            websocket.send_json(
                {
                    "id": 2,
                    "method": "quotes.unsubscribe",
                    "params": {"subscription_id": subscription_id},
                }
            )
            response = websocket.receive_json()
            assert response["result"] == {
                "subscription_id": subscription_id,
                "removed": True,
            }
