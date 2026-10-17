"""扩展行情同步 TCP 连接（端口 7727）与主机并发探活。"""

from __future__ import annotations

import socket
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from types import TracebackType
from typing import TYPE_CHECKING, Any, Callable, TypeVar

from eltdx.ex.commands.get_instrument_info import GetExInstrumentInfoCmd
from eltdx.ex.frame import HEADER_SIZE, decompress_body, parse_header
from eltdx.ex.hosts import DEFAULT_EX_HOSTS, DEFAULT_EX_PORT, load_ex_hosts
from eltdx.exceptions import EltdxError, TransportError

if TYPE_CHECKING:
    from eltdx.ex.commands.base import BaseCommand

_T = TypeVar("_T")

_DEFAULT_TIMEOUT = 15.0

DEFAULT_PROBE_TIMEOUT: float = 3.0
DEFAULT_PROBE_WORKERS: int = 16


def _first_host() -> str:
    return DEFAULT_EX_HOSTS[0]


@dataclass(frozen=True, slots=True)
class ExHostProbe:
    """单台扩展行情主机的能力探活结果。"""

    host: str
    ok: bool
    latency_ms: float | None = None
    error: str | None = None


def default_probe_command() -> "BaseCommand[Any]":
    """构造探活用命令：枚举第 1 条标的。

    刻意**不用** ``GetExInstrumentCountCmd``（``0x23f0``）：实测全部 16 台内置主机
    都应答它，毫无区分度——``ping_all()`` 于是会给出「16 台都健康」的错误结论，
    而真正能取数的只有极少数。``0x23f5``（枚举）只有应答业务命令的主机才回，
    正好用来筛掉「TCP 通、但不应答业务命令」的主机。
    """

    return GetExInstrumentInfoCmd(0, 1)


def probe_ex_host(
    host: str,
    port: int = DEFAULT_EX_PORT,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    *,
    command_factory: Callable[[], "BaseCommand[Any]"] = default_probe_command,
    mac_ex_mode: bool = False,
) -> ExHostProbe:
    """能力感知地探活单台主机：连得上**且**应答业务命令才算 ``ok``。

    失败不抛异常，统一返回 ``ExHostProbe(ok=False, error=...)``，便于并发汇总。
    """

    started = time.monotonic()
    conn = ExTdxConnection(host, port, timeout, mac_ex_mode=mac_ex_mode)
    try:
        conn.connect()
        conn.execute(command_factory())
    except (EltdxError, OSError) as exc:
        return ExHostProbe(host=host, ok=False, error=f"{type(exc).__name__}: {exc}")
    finally:
        conn.close()
    return ExHostProbe(
        host=host,
        ok=True,
        latency_ms=round((time.monotonic() - started) * 1000.0, 2),
    )


def probe_ex_hosts(
    hosts: list[str] | None = None,
    port: int = DEFAULT_EX_PORT,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    *,
    max_workers: int = DEFAULT_PROBE_WORKERS,
    command_factory: Callable[[], "BaseCommand[Any]"] = default_probe_command,
    mac_ex_mode: bool = False,
    stop_after_first: bool = False,
) -> list[ExHostProbe]:
    """并发探活多台主机；返回结果与输入顺序一一对应。

    ``hosts=None`` 表示「未指定」→ 用 :func:`eltdx.ex.hosts.load_ex_hosts`
    （环境变量 > 用户配置 > 包内默认）；``hosts=[]`` 表示「显式要零台」
    → 返回空列表（两者不可混淆）。

    ``stop_after_first=True`` 时，一旦某台主机确认应答就立即返回，不再等其余主机的
    超时。真实环境里 15/16 台主机是静默的，等待它们要把调用方拖满一个 ``timeout``
    （默认 3 秒），而可用主机通常 50 毫秒内就回——早退能把这段等待几乎完全省掉。
    代价是未探到的主机标记为 ``error="not probed"``（是「未知」而非「不可用」），
    需要完整排名时请保持默认的 ``False``。
    """

    candidates = load_ex_hosts() if hosts is None else list(hosts)
    if not candidates:
        return []

    workers = min(max(1, int(max_workers)), len(candidates))
    by_host: dict[str, ExHostProbe] = {}
    pool = ThreadPoolExecutor(max_workers=workers, thread_name_prefix="eltdx-ex-probe")
    try:
        futures = {
            pool.submit(
                probe_ex_host,
                host,
                port,
                timeout,
                command_factory=command_factory,
                mac_ex_mode=mac_ex_mode,
            ): host
            for host in candidates
        }
        for future in as_completed(futures):
            host = futures[future]
            try:
                result = future.result()
            except Exception as exc:  # pragma: no cover - 探活本身不应抛
                result = ExHostProbe(host=host, ok=False, error=type(exc).__name__)
            by_host[host] = result
            if stop_after_first and result.ok:
                break
    finally:
        # 早退时不等待在途探活，否则省下的等待又会在 shutdown 处补回来。
        pool.shutdown(wait=not stop_after_first, cancel_futures=stop_after_first)

    return [by_host.get(host, ExHostProbe(host=host, ok=False, error="not probed")) for host in candidates]


def rank_ex_hosts(
    results: list[ExHostProbe],
    *,
    candidates: list[str] | None = None,
) -> list[str]:
    """把探活结果排成连接优先级：健康主机（按延迟升序）在前，其余保持原顺序在后。

    保留不健康主机是有意的——网络抖动会让某台临时探活失败，直接丢弃可能让
    会话再也回不到它；排在后面既不影响首选，又留了退路。
    """

    ordered = candidates if candidates is not None else [result.host for result in results]
    healthy = sorted(
        (result for result in results if result.ok and result.latency_ms is not None),
        key=lambda result: result.latency_ms if result.latency_ms is not None else float("inf"),
    )
    healthy_hosts = [result.host for result in healthy]
    healthy_set = set(healthy_hosts)
    return healthy_hosts + [host for host in ordered if host not in healthy_set]


def ping_ex_host(
    host: str,
    port: int = DEFAULT_EX_PORT,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
) -> float | None:
    """测量单台扩展行情服务器延迟（秒）。

    与 0.1 版语义的差别：**不再只看 TCP 能否连上**。实测 16 台内置主机 TCP 全通，
    但只有少数会应答业务命令，所以「连得上但不应答」现在返回 ``None``。
    """

    result = probe_ex_host(host, port, timeout)
    if not result.ok or result.latency_ms is None:
        return None
    return result.latency_ms / 1000.0


def ping_ex_all(
    hosts: list[str] | None = None,
    port: int = DEFAULT_EX_PORT,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
) -> list[tuple[str, float]]:
    """并发探活多台主机，按延迟升序返回**应答业务命令**的主机。"""

    results = probe_ex_hosts(hosts, port, timeout)
    healthy = sorted(
        (result for result in results if result.ok and result.latency_ms is not None),
        key=lambda result: result.latency_ms if result.latency_ms is not None else float("inf"),
    )
    return [
        (result.host, result.latency_ms / 1000.0)
        for result in healthy
        if result.latency_ms is not None
    ]


def _recv_exact_sock(sock: socket.socket, n: int) -> bytes:
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise TransportError("连接被服务器关闭")
        buf.extend(chunk)
    return bytes(buf)


class ExTdxConnection:
    """扩展行情同步 TCP 连接（端口 7727，单包握手）。

    Parameters
    ----------
    mac_ex_mode : bool
        为 True 时自动把 MAC 命令的 head_flag 从 0x1C 转为 0x01，
        以兼容 MAC EX 服务器（要求 head_flag=0x01）。
    """

    def __init__(
        self,
        host: str | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = _DEFAULT_TIMEOUT,
        *,
        mac_ex_mode: bool = False,
    ) -> None:
        self.host = host if host is not None else _first_host()
        self.port = port
        self.timeout = timeout
        self.mac_ex_mode = mac_ex_mode
        self._sock: socket.socket | None = None
        self._lock = threading.Lock()

    def connect(self) -> None:
        """建立 TCP 连接；失败抛 TransportError。"""

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(self.timeout)
        try:
            sock.connect((self.host, self.port))
        except OSError as e:
            sock.close()
            raise TransportError(f"无法连接 {self.host}:{self.port}: {e}") from e
        self._sock = sock

    def close(self) -> None:
        """关闭连接（幂等）。"""

        if self._sock is not None:
            try:
                self._sock.close()
            except OSError:
                pass
            self._sock = None

    def execute(self, cmd: BaseCommand[_T]) -> _T:
        """执行一条命令：发送请求，接收并解压响应，返回解析结果。"""

        with self._lock:
            sock = self._require_sock()
            request = cmd.build_request()
            if self.mac_ex_mode and request and request[0] == 0x1C:
                request = b"\x01" + request[1:]
            try:
                sock.sendall(request)
                header = parse_header(self._recv_exact(HEADER_SIZE))
                raw_body = self._recv_exact(header.zipsize)
            except OSError as e:
                raise TransportError(f"通信错误: {e}") from e
            body = decompress_body(header, raw_body)
            return cmd.parse_response(body)

    def __enter__(self) -> "ExTdxConnection":
        self.connect()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        self.close()

    def _require_sock(self) -> socket.socket:
        if self._sock is None:
            raise TransportError("未连接，请先调用 connect()")
        return self._sock

    def _recv_exact(self, n: int) -> bytes:
        sock = self._require_sock()
        return _recv_exact_sock(sock, n)


__all__ = [
    "DEFAULT_PROBE_TIMEOUT",
    "DEFAULT_PROBE_WORKERS",
    "ExHostProbe",
    "ExTdxConnection",
    "default_probe_command",
    "ping_ex_all",
    "ping_ex_host",
    "probe_ex_host",
    "probe_ex_hosts",
    "rank_ex_hosts",
]
