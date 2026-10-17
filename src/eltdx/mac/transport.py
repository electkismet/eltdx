"""MAC 协议同步传输层（基于 socket）。

:class:`MacConnection` 负责 TCP 连接、三步握手、请求发送与响应接收/解压。
为便于离线测试，构造时可注入 ``socket_factory``（默认 :func:`socket.socket`），
测试传入斐波 socket 即可覆盖 setup/收发/超时路径，**无需真实联网**。
"""

from __future__ import annotations

import socket
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from types import TracebackType
from typing import TYPE_CHECKING, Any, Callable, Iterable, Protocol, TypeVar

from ..exceptions import (
    ConnectionClosedError,
    EltdxError,
    ResponseTimeoutError,
    TransportError,
)
from .codec import HEADER_SIZE, decompress_mac_body, parse_mac_header
from .hosts import MAC_HOSTS, MAC_PORT

if TYPE_CHECKING:
    from .commands import BaseCommand

T = TypeVar("T")

# 握手命令原始字节（与 easy_tdx 一致，已在真实服务器验证）。
SETUP_CMD1 = bytes.fromhex("0c0218930001030003000d0001")
SETUP_CMD2 = bytes.fromhex("0c0218940001030003000d0002")
SETUP_CMD3 = bytes.fromhex(
    "0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002"
)
SETUP_COMMANDS: tuple[bytes, ...] = (SETUP_CMD1, SETUP_CMD2, SETUP_CMD3)


class SocketLike(Protocol):
    """传输层使用的 socket 最小接口（便于测试注入）。"""

    def connect(self, address: tuple[str, int]) -> Any: ...

    def settimeout(self, value: float | None) -> None: ...

    def sendall(self, data: bytes) -> None: ...

    def recv(self, bufsize: int) -> bytes: ...

    def close(self) -> None: ...


SocketFactory = Callable[[int, int], SocketLike]


def _default_socket_factory(family: int, type_: int) -> SocketLike:
    return socket.socket(family, type_)


def _safe_close(sock: SocketLike) -> None:
    try:
        sock.close()
    except OSError:
        pass


def _recv_exact(sock: SocketLike, n: int) -> bytes:
    """循环 ``recv`` 直到读满 ``n`` 字节；连接被关闭时抛 :class:`ConnectionClosedError`。"""

    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionClosedError("连接被服务器关闭")
        buf.extend(chunk)
    return bytes(buf)


class MacConnection:
    """MAC 协议同步 TCP 连接。

    使用示例::

        with MacConnection("121.36.248.138") as conn:
            result = conn.execute(SymbolQuotesCmd([(0, "600000")]))
    """

    def __init__(
        self,
        host: str,
        port: int,
        timeout: float,
        *,
        socket_factory: SocketFactory | None = None,
        setup_commands: tuple[bytes, ...] = SETUP_COMMANDS,
    ) -> None:
        self.host = host
        self.port = port
        self.timeout = timeout
        self._socket_factory: SocketFactory = socket_factory or _default_socket_factory
        self._setup_commands = setup_commands
        self._sock: SocketLike | None = None

    @property
    def connected(self) -> bool:
        """当前是否已建立连接。"""

        return self._sock is not None

    def connect(self) -> None:
        """建立 TCP 连接并完成三步握手。"""

        sock = self._socket_factory(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(self.timeout)
        try:
            sock.connect((self.host, self.port))
        except OSError as exc:
            _safe_close(sock)
            raise TransportError(f"无法连接 {self.host}:{self.port}: {exc}") from exc
        self._sock = sock
        try:
            self._send_setup()
        except Exception:
            _safe_close(sock)
            self._sock = None
            raise

    def close(self) -> None:
        """关闭连接（幂等）。"""

        if self._sock is not None:
            _safe_close(self._sock)
            self._sock = None

    def execute(self, cmd: "BaseCommand[T]") -> T:
        """执行一条命令：发送请求、接收并解压响应、返回解析结果。"""

        if self._sock is None:
            raise TransportError("未连接，请先调用 connect()")
        request = cmd.build_request()
        try:
            self._sock.sendall(request)
            header_buf = self._recv_exact(HEADER_SIZE)
            header = parse_mac_header(header_buf)
            raw_body = self._recv_exact(header.zipsize)
        except socket.timeout as exc:
            raise ResponseTimeoutError(f"请求超时: {exc}") from exc
        except OSError as exc:
            raise ConnectionClosedError(f"通信错误: {exc}") from exc
        body = decompress_mac_body(header, raw_body)
        return cmd.parse_response(body)

    def __enter__(self) -> "MacConnection":
        self.connect()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        self.close()

    def _send_setup(self) -> None:
        """按序发送握手命令并丢弃响应（部分服务器握手无响应，忽略其错误）。"""

        for cmd_bytes in self._setup_commands:
            assert self._sock is not None
            self._sock.sendall(cmd_bytes)
            try:
                hdr_buf = self._recv_exact(HEADER_SIZE)
                hdr = parse_mac_header(hdr_buf)
                if hdr.zipsize > 0:
                    self._recv_exact(hdr.zipsize)
            except (OSError, TransportError):
                pass

    def _recv_exact(self, n: int) -> bytes:
        assert self._sock is not None
        return _recv_exact(self._sock, n)


def ping_mac_host(
    host: str,
    port: int = MAC_PORT,
    timeout: float = 5.0,
    *,
    socket_factory: SocketFactory | None = None,
) -> float | None:
    """测量单台 MAC 行情服务器延迟（秒），失败返回 ``None``。

    判定标准不只是「TCP 连得上」：完成三步握手后还要真正发一条
    :class:`~eltdx.mac.commands.ServerInfoCmd`，并且服务器要**给出交易日**。
    只靠「没抛异常」不够——``ServerInfoCmd`` 对过短的响应体会宽容地返回
    ``ServerSession(today="")``，所以这里额外要求 ``today`` 非空，
    否则端口开放但不提供 MAC 行情的主机会被误选为最佳节点。

    ``socket_factory`` 与 :class:`MacConnection` 同义，仅用于离线测试注入。
    """

    # 局部导入：transport 位于 commands 之下，避免模块级导入顺序耦合。
    from .commands import ServerInfoCmd

    started = time.monotonic()
    connection = MacConnection(host, port, timeout, socket_factory=socket_factory)
    try:
        connection.connect()
        session = connection.execute(ServerInfoCmd())
        if not session.today:
            return None
    except (OSError, EltdxError):
        return None
    finally:
        connection.close()
    return time.monotonic() - started


def ping_mac_all(
    hosts: Iterable[str] | None = None,
    port: int = MAC_PORT,
    timeout: float = 5.0,
    *,
    socket_factory: SocketFactory | None = None,
) -> list[tuple[str, float]]:
    """并发测量多台 MAC 行情服务器延迟，按延迟升序返回 ``(host, seconds)``。

    探测失败的主机不会出现在结果里；全部失败时返回空列表。
    """

    candidates = [str(h) for h in hosts] if hosts else list(MAC_HOSTS)
    if not candidates:
        return []
    results: list[tuple[str, float]] = []
    with ThreadPoolExecutor(max_workers=len(candidates)) as pool:
        futures = {
            pool.submit(
                ping_mac_host, h, port, timeout, socket_factory=socket_factory
            ): h
            for h in candidates
        }
        for future in as_completed(futures):
            latency = future.result()
            if latency is not None:
                results.append((futures[future], latency))
    results.sort(key=lambda item: item[1])
    return results


__all__ = [
    "SETUP_CMD1",
    "SETUP_CMD2",
    "SETUP_CMD3",
    "SETUP_COMMANDS",
    "MacConnection",
    "SocketFactory",
    "SocketLike",
    "ping_mac_all",
    "ping_mac_host",
]