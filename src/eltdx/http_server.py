"""Optional FastAPI gateway for the public :mod:`eltdx` APIs.

The gateway is deliberately kept outside the core package surface.  Importing
``eltdx`` does not require FastAPI or Uvicorn; those dependencies are loaded
only when :func:`create_app` or :func:`main` is used.

HTTP and WebSocket requests share one ``TdxClient`` instance.  HTTP is useful
for request/response calls, while the WebSocket endpoint additionally exposes
``quotes.subscribe`` for the native ``0x0547`` push queue.
"""

import argparse
import asyncio
import contextlib
import inspect
import logging
import uuid
from collections import deque
from collections.abc import Mapping, Sequence
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from typing import Any, Callable

from .client import TdxClient
from .exceptions import EltdxError, TransportError
from .protocol.unit import normalize_code
from .serialization import to_jsonable

LOGGER = logging.getLogger(__name__)

_API_ROOTS = (
    "session",
    "codes",
    "quotes",
    "resources",
    "bars",
    "minutes",
    "trades",
    "auctions",
    "corporate",
    "limits",
    "workdays",
    "f10",
    "helpers",
)
_TOP_LEVEL_METHODS = frozenset({"ping", "clear_cache"})
_PUSH_METHOD = "quotes.subscribe"
_UNSUBSCRIBE_METHOD = "quotes.unsubscribe"
_MAX_QUOTE_SUBSCRIPTION_CODES = 100
_MAX_WS_QUEUE = 256
_MAX_WS_RESPONSES = 32


class GatewayRequestError(ValueError):
    """Raised when an HTTP or WebSocket RPC envelope is malformed."""


class GatewayMethodError(GatewayRequestError):
    """Raised when an RPC method is not part of the public client surface."""


@dataclass(frozen=True, slots=True)
class RpcRequest:
    request_id: Any
    method: str
    params: Mapping[str, Any] | Sequence[Any]


@dataclass(slots=True)
class _WebSocketSession:
    """Keep RPC replies reliable while bounding lossy quote events."""

    websocket: Any
    responses: asyncio.Queue[dict[str, Any]] = field(
        default_factory=lambda: asyncio.Queue(maxsize=_MAX_WS_RESPONSES)
    )
    events: deque[dict[str, Any]] = field(
        default_factory=lambda: deque(maxlen=_MAX_WS_QUEUE)
    )
    ready: asyncio.Event = field(default_factory=asyncio.Event)
    closed: bool = False

    async def send(self, message: dict[str, Any]) -> None:
        if not self.closed:
            await self.responses.put(message)
            self.ready.set()

    def send_event_nowait(self, message: dict[str, Any]) -> None:
        if self.closed:
            return
        # deque(maxlen=...) discards only the oldest quote event. RPC replies
        # use a separate reliable queue and are never displaced by market data.
        self.events.append(message)
        self.ready.set()

    async def writer(self) -> None:
        while True:
            await self.ready.wait()
            while True:
                try:
                    message = self.responses.get_nowait()
                except asyncio.QueueEmpty:
                    if self.events:
                        message = self.events.popleft()
                    else:
                        self.ready.clear()
                        if self.closed:
                            return
                        break
                await self.websocket.send_json(message)

    def close(self) -> None:
        self.closed = True
        self.ready.set()


@dataclass(frozen=True, slots=True)
class _QuoteSubscriber:
    subscription_id: str
    session: _WebSocketSession
    codes: frozenset[str]


class _QuoteHub:
    """Fan out native 0x0547 push records to WebSocket subscribers."""

    def __init__(self, client: TdxClient) -> None:
        self._client = client
        self._subscribers: dict[str, _QuoteSubscriber] = {}
        self._lock = asyncio.Lock()
        self._pump_task: asyncio.Task[None] | None = None

    async def subscribe(
        self, session: _WebSocketSession, codes: Sequence[str]
    ) -> tuple[str, Any]:
        normalized = _normalize_codes(
            codes, maximum=_MAX_QUOTE_SUBSCRIPTION_CODES
        )
        # Establish the 0x0547 baseline before registering the subscription.
        # The returned page is useful to the caller immediately; subsequent
        # updates arrive as quote events.
        baseline = await asyncio.to_thread(self._client.quotes.get_depth, normalized)
        subscription_id = uuid.uuid4().hex
        subscriber = _QuoteSubscriber(
            subscription_id=subscription_id,
            session=session,
            codes=frozenset(normalized),
        )
        async with self._lock:
            self._subscribers[subscription_id] = subscriber
            if self._pump_task is None or self._pump_task.done():
                self._pump_task = asyncio.create_task(
                    self._pump(), name="eltdx-quote-pump"
                )
        return subscription_id, baseline

    async def unsubscribe(self, subscription_id: str) -> bool:
        async with self._lock:
            removed = self._subscribers.pop(subscription_id, None) is not None
            should_stop = not self._subscribers
            task = self._pump_task if should_stop else None
            if should_stop:
                self._pump_task = None
        if task is not None:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task
        return removed

    async def remove_session(self, session: _WebSocketSession) -> None:
        async with self._lock:
            ids = [
                subscription_id
                for subscription_id, subscriber in self._subscribers.items()
                if subscriber.session is session
            ]
        for subscription_id in ids:
            await self.unsubscribe(subscription_id)

    async def close(self) -> None:
        async with self._lock:
            self._subscribers.clear()
            task = self._pump_task
            self._pump_task = None
        if task is not None:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task

    async def _pump(self) -> None:
        while True:
            async with self._lock:
                if not self._subscribers:
                    return
                subscribers = tuple(self._subscribers.values())

            try:
                frame = await asyncio.to_thread(
                    self._client.quotes.poll_push,
                    timeout=0.5,
                    parse=True,
                )
            except Exception as exc:  # keep other subscriptions alive
                LOGGER.warning("eltdx quote push poll failed: %s", exc)
                for subscriber in subscribers:
                    subscriber.session.send_event_nowait(
                        {
                            "event": "error",
                            "subscription_id": subscriber.subscription_id,
                            "error": {
                                "type": type(exc).__name__,
                                "message": str(exc),
                            },
                        }
                    )
                await asyncio.sleep(0.1)
                continue

            if frame is None:
                # Some test/custom transports return immediately instead of
                # honoring the timeout. Avoid a busy loop in that case.
                await asyncio.sleep(0.01)
                continue
            records = tuple(getattr(frame, "records", ()) or ())
            if not records:
                continue
            for record in records:
                full_code = getattr(record, "full_code", None)
                if not isinstance(full_code, str):
                    continue
                for subscriber in subscribers:
                    if full_code in subscriber.codes:
                        subscriber.session.send_event_nowait(
                            {
                                "event": "quote",
                                "subscription_id": subscriber.subscription_id,
                                "data": _jsonable(record),
                            }
                        )


class _Gateway:
    """Method resolver and JSON conversion shared by both transports."""

    def __init__(self, client: TdxClient) -> None:
        self.client = client

    def call(
        self,
        method: str,
        params: Mapping[str, Any] | Sequence[Any] | None = None,
    ) -> Any:
        target = self.resolve(method)
        values: Mapping[str, Any] | Sequence[Any]
        if params is None:
            values = {}
        elif isinstance(params, Mapping):
            values = params
        elif isinstance(params, Sequence) and not isinstance(
            params, (str, bytes, bytearray)
        ):
            values = params
        else:
            raise GatewayRequestError("params must be an object or an array")

        try:
            if isinstance(values, Mapping):
                return target(**dict(values))
            return target(*list(values))
        except TypeError as exc:
            raise GatewayRequestError(str(exc)) from exc

    def resolve(self, method: str) -> Callable[..., Any]:
        if not isinstance(method, str) or not method.strip():
            raise GatewayRequestError("method must be a non-empty string")
        method = method.strip()
        parts = method.split(".")
        if any(not part or part.startswith("_") for part in parts):
            raise GatewayMethodError(f"unknown method: {method}")

        if len(parts) == 1:
            if parts[0] not in _TOP_LEVEL_METHODS:
                raise GatewayMethodError(f"unknown method: {method}")
            target = getattr(self.client, parts[0], None)
        elif len(parts) == 2 and parts[0] in _API_ROOTS:
            target = getattr(getattr(self.client, parts[0], None), parts[1], None)
        else:
            raise GatewayMethodError(f"unknown method: {method}")

        if not callable(target):
            raise GatewayMethodError(f"unknown method: {method}")
        return target

    def call_json(
        self,
        method: str,
        params: Mapping[str, Any] | Sequence[Any] | None = None,
    ) -> Any:
        """Run a public call and finish any lazy JSON conversion off-loop."""

        return _jsonable(self.call(method, params))

    def methods(self) -> list[str]:
        names = set(_TOP_LEVEL_METHODS)
        for root in _API_ROOTS:
            service = getattr(self.client, root, None)
            if service is None:
                continue
            for name in dir(service):
                if name.startswith("_"):
                    continue
                try:
                    value = getattr(service, name)
                except (AttributeError, TypeError):
                    # Python 3.10 can expose dataclass slots descriptors in
                    # dir(instance) that are invalid for this concrete object.
                    continue
                if callable(value):
                    names.add(f"{root}.{name}")
        return sorted(names)


def create_app(
    *,
    client: TdxClient | None = None,
    host: str | None = None,
    hosts: Sequence[str] | None = None,
    timeout: float = 8.0,
    pool_size: int | None = None,
    server_count: int = 2,
    connections_per_server: int | None = None,
    heartbeat_interval: float | None = 30.0,
) -> Any:
    """Create the optional FastAPI application.

    ``client`` is primarily intended for tests or applications that already
    own a configured client.  When omitted, the app creates one client at
    startup and closes it during shutdown.  The client is shared by all HTTP
    requests and WebSocket connections in this process.
    """

    try:
        from fastapi import Body, FastAPI, WebSocket, WebSocketDisconnect
        from fastapi.responses import JSONResponse
    except ImportError as exc:  # pragma: no cover - depends on optional extras
        raise RuntimeError(
            "FastAPI gateway requires optional dependencies. "
            "Install with: pip install 'eltdx[http]'"
        ) from exc

    runtime_client = client
    owns_client = client is None
    gateway: _Gateway | None = None
    hub: _QuoteHub | None = None

    @asynccontextmanager
    async def lifespan(_app: Any):
        nonlocal runtime_client, gateway, hub
        if runtime_client is None:
            runtime_client = TdxClient(
                host=host,
                hosts=hosts,
                timeout=timeout,
                pool_size=pool_size,
                server_count=server_count,
                connections_per_server=connections_per_server,
                heartbeat_interval=heartbeat_interval,
            )
            await asyncio.to_thread(runtime_client.connect)
        gateway = _Gateway(runtime_client)
        hub = _QuoteHub(runtime_client)
        try:
            yield
        finally:
            assert hub is not None
            await hub.close()
            if owns_client and runtime_client is not None:
                await asyncio.to_thread(runtime_client.close)

    app = FastAPI(
        title="eltdx HTTP/WebSocket gateway",
        version=str(_package_version()),
        description=(
            "HTTP JSON and WebSocket RPC access to the public eltdx APIs. "
            "Use quotes.subscribe for native 0x0547 quote updates."
        ),
        lifespan=lifespan,
    )

    def active_gateway() -> _Gateway:
        if gateway is None:
            raise RuntimeError("eltdx gateway has not started")
        return gateway

    @app.get("/health")
    async def health() -> dict[str, Any]:
        return {
            "ok": True,
            "service": "eltdx",
            "version": str(_package_version()),
            "transports": ["http", "websocket"],
            "native_push_method": "quotes.subscribe",
        }

    @app.get("/methods")
    async def methods() -> dict[str, Any]:
        return {
            "methods": active_gateway().methods(),
            "websocket_only": [_PUSH_METHOD, _UNSUBSCRIBE_METHOD],
        }

    @app.post("/rpc")
    async def rpc(body: Any = Body(...)) -> Any:
        try:
            request = _parse_request(body)
            result = await asyncio.to_thread(
                active_gateway().call_json, request.method, request.params
            )
            return {
                "id": request.request_id,
                "ok": True,
                "result": result,
            }
        except Exception as exc:
            return JSONResponse(
                status_code=_status_code(exc),
                content=_error_response(
                    body.get("id") if isinstance(body, Mapping) else None, exc
                ),
            )

    @app.websocket("/ws")
    async def websocket_rpc(websocket: WebSocket):
        await websocket.accept()
        session = _WebSocketSession(websocket)
        writer = asyncio.create_task(session.writer(), name="eltdx-ws-writer")
        try:
            while True:
                body: Any = None
                try:
                    body = await websocket.receive_json()
                    request = _parse_request(body)
                    if request.method == _PUSH_METHOD:
                        result = await _subscribe(hub, session, request.params)
                    elif request.method == _UNSUBSCRIBE_METHOD:
                        result = await _unsubscribe(hub, request.params)
                    else:
                        result = await asyncio.to_thread(
                            active_gateway().call_json,
                            request.method,
                            request.params,
                        )
                    await session.send(
                        {
                            "id": request.request_id,
                            "ok": True,
                            "result": result,
                        }
                    )
                except WebSocketDisconnect:
                    raise
                except Exception as exc:
                    request_id = (
                        body.get("id")
                        if isinstance(body, Mapping)
                        else None
                    )
                    await session.send(_error_response(request_id, exc))
        except WebSocketDisconnect:
            pass
        finally:
            session.close()
            if hub is not None:
                await hub.remove_session(session)
            writer.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await writer

    return app


async def _subscribe(
    hub: _QuoteHub | None,
    session: _WebSocketSession,
    params: Mapping[str, Any] | Sequence[Any],
) -> dict[str, Any]:
    if hub is None:
        raise RuntimeError("eltdx gateway has not started")
    if not isinstance(params, Mapping):
        raise GatewayRequestError("quotes.subscribe params must be an object")
    codes = params.get("codes")
    if isinstance(codes, str):
        code_list = [codes]
    elif isinstance(codes, Sequence) and not isinstance(
        codes, (bytes, bytearray, str)
    ):
        code_list = list(codes)
    else:
        raise GatewayRequestError("quotes.subscribe requires a codes array")
    subscription_id, baseline = await hub.subscribe(session, code_list)
    return {
        "subscription_id": subscription_id,
        "codes": _normalize_codes(
            code_list, maximum=_MAX_QUOTE_SUBSCRIPTION_CODES
        ),
        "initial": _jsonable(baseline),
    }


async def _unsubscribe(
    hub: _QuoteHub | None,
    params: Mapping[str, Any] | Sequence[Any],
) -> dict[str, Any]:
    if hub is None:
        raise RuntimeError("eltdx gateway has not started")
    if not isinstance(params, Mapping):
        raise GatewayRequestError("quotes.unsubscribe params must be an object")
    subscription_id = params.get("subscription_id")
    if not isinstance(subscription_id, str) or not subscription_id:
        raise GatewayRequestError(
            "quotes.unsubscribe requires a subscription_id string"
        )
    return {"subscription_id": subscription_id, "removed": await hub.unsubscribe(subscription_id)}


def _parse_request(body: Any) -> RpcRequest:
    if not isinstance(body, Mapping):
        raise GatewayRequestError("RPC body must be a JSON object")
    method = body.get("method")
    if not isinstance(method, str) or not method.strip():
        raise GatewayRequestError("method must be a non-empty string")
    params = body.get("params", {})
    if params is None:
        params = {}
    if not isinstance(params, Mapping) and not (
        isinstance(params, Sequence)
        and not isinstance(params, (str, bytes, bytearray))
    ):
        raise GatewayRequestError("params must be an object or an array")
    return RpcRequest(body.get("id"), method.strip(), params)


def _normalize_codes(codes: Sequence[str], *, maximum: int) -> list[str]:
    if not codes:
        raise GatewayRequestError("codes must not be empty")
    if len(codes) > maximum:
        raise GatewayRequestError(f"codes accepts at most {maximum} securities")
    result: list[str] = []
    for code in codes:
        if not isinstance(code, str) or not code.strip():
            raise GatewayRequestError("each security code must be a non-empty string")
        try:
            normalized = normalize_code(code)
        except Exception as exc:
            raise GatewayRequestError(str(exc)) from exc
        if normalized not in result:
            result.append(normalized)
    return result


def _jsonable(value: Any) -> Any:
    """Convert models plus the one generator-returning helper to JSON."""

    if inspect.isgenerator(value) or isinstance(value, (map, filter)):
        return to_jsonable(list(value))
    return to_jsonable(value)


def _status_code(exc: BaseException) -> int:
    if isinstance(exc, GatewayMethodError):
        return 404
    if isinstance(exc, GatewayRequestError) or isinstance(exc, (TypeError, ValueError)):
        return 400
    if isinstance(exc, (TransportError, TimeoutError, OSError)):
        return 502
    if isinstance(exc, EltdxError):
        return 502
    return 500


def _error_response(request_id: Any, exc: BaseException) -> dict[str, Any]:
    return {
        "id": request_id,
        "ok": False,
        "error": {"type": type(exc).__name__, "message": str(exc)},
    }


def _package_version() -> str:
    from . import __version__

    return __version__


def main(argv: Sequence[str] | None = None) -> int:
    """Run the optional FastAPI gateway with Uvicorn."""

    parser = argparse.ArgumentParser(description="Run the eltdx FastAPI gateway")
    parser.add_argument("--host", default="127.0.0.1", help="HTTP listen address")
    parser.add_argument("--port", type=int, default=8000, help="HTTP listen port")
    parser.add_argument("--tdx-host", default=None, help="one 7709 host:port")
    parser.add_argument(
        "--tdx-hosts",
        default=None,
        help="comma-separated 7709 host:port candidates",
    )
    parser.add_argument("--timeout", type=float, default=8.0)
    parser.add_argument("--pool-size", type=int, default=None)
    parser.add_argument("--server-count", type=int, default=2)
    parser.add_argument("--connections-per-server", type=int, default=None)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args(argv)

    try:
        import uvicorn
    except ImportError as exc:  # pragma: no cover - depends on optional extras
        raise RuntimeError(
            "Uvicorn is required to run the gateway. "
            "Install with: pip install 'eltdx[http]'"
        ) from exc

    hosts = None
    if args.tdx_hosts:
        hosts = tuple(item.strip() for item in args.tdx_hosts.split(",") if item.strip())
    app = create_app(
        host=args.tdx_host,
        hosts=hosts,
        timeout=args.timeout,
        pool_size=args.pool_size,
        server_count=args.server_count,
        connections_per_server=args.connections_per_server,
    )
    uvicorn.run(app, host=args.host, port=args.port, log_level=args.log_level)
    return 0


__all__ = ["create_app", "main"]


if __name__ == "__main__":
    raise SystemExit(main())
