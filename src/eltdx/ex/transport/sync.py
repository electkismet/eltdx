"""扩展行情同步 TCP 连接（端口 7727）与主机并发探活。"""

from __future__ import annotations

import socket
import threading
import time
from sys import modules as _MODULES
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from types import TracebackType
from typing import TYPE_CHECKING, Any, Callable, TypeVar

from eltdx.ex.commands.get_markets import GetExMarketsCmd
from eltdx.ex.frame import HEADER_SIZE, decompress_body, parse_header
from eltdx.ex.hosts import DEFAULT_EX_HOSTS, DEFAULT_EX_PORT, load_ex_hosts
from eltdx.exceptions import EltdxError, TransportError

if TYPE_CHECKING:
    from eltdx.ex.commands.base import BaseCommand

_T = TypeVar("_T")

_DEFAULT_TIMEOUT = 15.0

# pytdx TdxExHq_API.setup() / ExSetupCmd1 同款 92B 固定会话握手包。
# DATA 级主站未收到此包时不回应数据类命令（markets / quote / bars），
# 只回应 0x23f0 商品总数；见 electkismet/eltdx#28。
_EX_SETUP_PKG = bytes.fromhex(
    "010148650001520052005424"
    + "1f32c6e5d53dfb41" * 8
    + "cce16dffd5ba3fb8cbc57a054f7748ea"
)

DEFAULT_PROBE_TIMEOUT: float = 3.0
DEFAULT_PROBE_WORKERS: int = 16


def _first_host() -> str:
    from eltdx._native_abi import load_native
    return load_native().extension_probe_default("first", _MODULES[__name__])


def _native_probe_values(values):
    # Preserve generator protocol for custom sorted adapters.
    yield from values


def _native_probe_latency(result):
    from eltdx._native_abi import load_native
    return load_native().extension_probe_key(result, _MODULES[__name__])


def _native_probe_future_result(future, host):
    from eltdx._native_abi import load_native
    try:
        return future.result()
    except Exception as exc:
        return load_native().extension_probe_error(host, exc, False, _MODULES[__name__])


@dataclass(frozen=True, slots=True)
class ExHostProbe:
    """单台扩展行情主机的能力探活结果。"""

    host: str
    ok: bool
    latency_ms: float | None = None
    error: str | None = None


def default_probe_command() -> "BaseCommand[Any]":
    """构造探活用命令：市场列表（0x23f4）。

    ``0x23f0``（商品总数）16 台内置主机全部应答，毫无区分度；``0x23f5``（枚举）
    在未完成会话握手时 DATA 级供数主站不应答，反而会筛掉真正能取数的主机。
    完成 92B ExSetupCmd1 握手后，DATA 级主站应答 ``0x23f4``，目录级主站断开，
    因此市场列表是区分「是否真正供数」的最佳探活命令。
    """

    from eltdx._native_abi import load_native
    return load_native().extension_probe_default("command", _MODULES[__name__])


class _ProbeRegistry:
    """登记在途的探活连接，供早退时立刻中断它们。

    ``ThreadPoolExecutor`` 的 ``cancel_futures`` 只能取消**尚未开始**的任务；已经卡在
    TCP 握手里的线程若放着不管，要等满 ``timeout`` 才退出——短时间内反复创建客户端
    就会把进程线程数顶上去。把连接登记下来、早退时逐个 ``close``，阻塞中的
    ``connect()`` 会立刻失败，线程随即回收。
    """

    __slots__ = ("_lock", "_connections")

    def __init__(self) -> None:
        from eltdx._native_abi import load_native
        load_native().extension_registry_init(self, _MODULES[__name__])

    def add(self, connection: ExTdxConnection) -> None:
        with self._lock:
            from eltdx._native_abi import load_native
            load_native().extension_registry_change("add", self, connection)

    def discard(self, connection: ExTdxConnection) -> None:
        with self._lock:
            try:
                from eltdx._native_abi import load_native
                load_native().extension_registry_change("discard", self, connection)
            except ValueError:
                pass

    def abort_all(self) -> None:
        """关闭所有在途连接；幂等，可由任意线程调用。"""

        from eltdx._native_abi import load_native
        with self._lock:
            connections = load_native().extension_registry_take(self)
        load_native().extension_registry_abort(connections)


def probe_ex_host(
    host: str,
    port: int = DEFAULT_EX_PORT,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    *,
    command_factory: Callable[[], "BaseCommand[Any]"] = default_probe_command,
    mac_ex_mode: bool = False,
    registry: _ProbeRegistry | None = None,
) -> ExHostProbe:
    """能力感知地探活单台主机：连得上**且**应答业务命令才算 ``ok``。

    失败不抛异常，统一返回 ``ExHostProbe(ok=False, error=...)``，便于并发汇总。
    ``registry`` 非空时会把连接登记进去，使调用方可以中断本次探活。
    """

    from eltdx._native_abi import load_native
    started, conn = load_native().extension_probe_begin(
        "ex", (host, port, timeout, mac_ex_mode, registry), _MODULES[__name__]
    )
    try:
        load_native().extension_probe_run(conn, command_factory, False)
    except (EltdxError, OSError) as exc:
        return load_native().extension_probe_error(host, exc, True, _MODULES[__name__])
    finally:
        load_native().extension_probe_close(conn, registry)
    return load_native().extension_probe_success("ex", started, host, _MODULES[__name__])


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
    早退时会**主动中断**在途连接（见 :class:`_ProbeRegistry`），所以不会留下一批要
    等超时才退出的线程；未探到的主机标记为 ``error="not probed"``（是「未知」而非
    「不可用」），需要完整排名时请保持默认的 ``False``。
    """

    from eltdx._native_abi import load_native
    prepared = load_native().extension_probe_prepare("ex", hosts, max_workers, _MODULES[__name__])
    if prepared is None:
        return []
    candidates, by_host, registry, pool = prepared
    try:
        load_native().extension_probe_collect(
            "ex", candidates, by_host, registry, pool,
            (port, timeout, command_factory, mac_ex_mode, stop_after_first), _MODULES[__name__],
        )
    finally:
        load_native().extension_probe_cleanup(registry, pool)
    return load_native().extension_probe_finish("ex", candidates, by_host, _MODULES[__name__])


def rank_ex_hosts(
    results: list[ExHostProbe],
    *,
    candidates: list[str] | None = None,
) -> list[str]:
    """把探活结果排成连接优先级：健康主机（按延迟升序）在前，其余保持原顺序在后。

    保留不健康主机是有意的——网络抖动会让某台临时探活失败，直接丢弃可能让
    会话再也回不到它；排在后面既不影响首选，又留了退路。
    """

    from eltdx._native_abi import load_native
    return load_native().extension_probe_rank(results, candidates, _MODULES[__name__])


def ping_ex_host(
    host: str,
    port: int = DEFAULT_EX_PORT,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    *,
    command_factory: Callable[[], "BaseCommand[Any]"] = default_probe_command,
    mac_ex_mode: bool = False,
) -> float | None:
    """测量单台扩展行情服务器延迟（秒）。

    与 0.1 版语义的差别：**不再只看 TCP 能否连上**。实测 16 台内置主机 TCP 全通，
    但只有少数会应答业务命令，所以「连得上但不应答」现在返回 ``None``。
    ``mac_ex_mode=True`` 时按 MAC-EX 会话探活：跳过 EX 92B 握手，改用登录/业务命令。
    """

    from eltdx._native_abi import load_native
    return load_native().extension_probe_ping(
        False, (host, port, timeout, command_factory, mac_ex_mode), _MODULES[__name__]
    )


def ping_ex_all(
    hosts: list[str] | None = None,
    port: int = DEFAULT_EX_PORT,
    timeout: float = DEFAULT_PROBE_TIMEOUT,
    *,
    command_factory: Callable[[], "BaseCommand[Any]"] = default_probe_command,
    mac_ex_mode: bool = False,
) -> list[tuple[str, float]]:
    """并发探活多台主机，按延迟升序返回**应答业务命令**的主机。

    ``mac_ex_mode=True`` 时按 MAC-EX 会话探活：跳过 EX 92B 握手，改用登录命令。
    """

    from eltdx._native_abi import load_native
    return load_native().extension_probe_ping(
        True, (hosts, port, timeout, command_factory, mac_ex_mode), _MODULES[__name__]
    )


def _recv_exact_sock(sock: socket.socket, n: int) -> bytes:
    from eltdx._native_abi import load_native
    return load_native().extension_recv_exact("ex", sock, n, _MODULES[__name__])


def _native_safe_close(sock):
    try:
        sock.close()
    except OSError:
        pass


def _native_signal_close(sock):
    """Signal cleanup is tied to the captured socket, not a later replacement."""
    try:
        shutdown = getattr(sock, "shutdown", None)
        if shutdown is not None:
            try:
                shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
    finally:
        _native_safe_close(sock)


class ExTdxConnection:
    """扩展行情同步 TCP 连接（端口 7727，92B 会话握手）。

    Parameters
    ----------
    mac_ex_mode : bool
        为 True 时自动把 MAC 命令的 head_flag 从 0x1C 转为 0x01，
        以兼容 MAC EX 服务器（要求 head_flag=0x01）。
    send_setup : bool
        是否在 TCP 建连后发送 92B ``ExSetupCmd1`` 会话握手包并读走应答。
        DATA 级供数主站缺少此包时不应答任何数据类命令，默认开启；
        MAC-EX 会话台另有 ``0x2454`` 登录流程，默认跳过握手。
    """

    def __init__(
        self,
        host: str | None = None,
        port: int = DEFAULT_EX_PORT,
        timeout: float = _DEFAULT_TIMEOUT,
        *,
        mac_ex_mode: bool = False,
        send_setup: bool = True,
    ) -> None:
        from eltdx._native_abi import load_native
        load_native().extension_connection_init("ex", self, (host, port, timeout, mac_ex_mode, send_setup), _MODULES[__name__])

    def connect(self) -> None:
        """建立 TCP 连接并完成 exhq 会话握手；失败抛 TransportError。

        socket 在 ``connect()`` **之前**就登记到 ``self._sock``：这样别的线程调用
        :meth:`close` 可以中断正卡在 TCP 握手里的本次调用。探活早退时正是靠这一点
        立刻回收在途线程（见 :func:`probe_ex_hosts`），否则它们要一直等到超时。

        握手的应答必须**整帧读走**（帧头 + 压缩体）。只发不读会让下一条业务命令
        把 299B 握手帧当成自己的响应解析，静默返回空结果或报错——这是旧文档里
        「发了握手之后服务器不再应答」结论的真实成因（流错位，而非服务端弃连）。
        """

        from eltdx._native_abi import load_native
        sock = load_native().extension_socket_prepare("ex", self, _MODULES[__name__])
        try:
            operation = load_native().extension_operation_begin(self, sock, _MODULES[__name__])
        except BaseException:
            self._sock = None
            _native_safe_close(sock)
            load_native().extension_generation_retire(self)
            raise
        try:
            try:
                load_native().extension_socket_connect("ex", self, sock, _MODULES[__name__])
            except (OSError, EltdxError) as e:
                self._sock = None
                try:
                    sock.close()
                except OSError:
                    pass
                load_native().extension_generation_retire(self)
                raise load_native().extension_transport_error("ex_connect", self, e, _MODULES[__name__]) from e
            load_native().extension_operation_finish(operation, None, _MODULES[__name__])
        except BaseException as exc:
            load_native().extension_operation_failed(operation, self, exc, _MODULES[__name__])
            raise

    def close(self) -> None:
        """关闭连接（幂等）。"""

        from eltdx._native_abi import load_native
        load_native().extension_connection_close("ex", self, _MODULES[__name__])

    def execute(self, cmd: BaseCommand[_T]) -> _T:
        """执行一条命令：发送请求，接收并解压响应，返回解析结果。"""

        with self._lock:
            sock = self._require_sock()
            from eltdx._native_abi import load_native
            operation = load_native().extension_operation_begin(self, sock, _MODULES[__name__])
            try:
                request = load_native().extension_request_prepare("ex", self, cmd, _MODULES[__name__])
                load_native().extension_operation_check(operation, _MODULES[__name__])
                try:
                    header, raw_body = load_native().extension_exchange("ex", self, sock, request, _MODULES[__name__])
                except OSError as e:
                    raise load_native().extension_transport_error("ex_io", self, e, _MODULES[__name__]) from e
                load_native().extension_operation_check(operation, _MODULES[__name__])
                result = load_native().extension_response_finish("ex", cmd, header, raw_body, _MODULES[__name__])
                return load_native().extension_operation_finish(operation, result, _MODULES[__name__])
            except BaseException as exc:
                load_native().extension_operation_failed(operation, self, exc, _MODULES[__name__])
                raise

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
        from eltdx._native_abi import load_native
        return load_native().extension_require_socket(self, _MODULES[__name__], False, __debug__)

    def _recv_exact(self, n: int) -> bytes:
        from eltdx._native_abi import load_native
        return load_native().extension_connection_receive("ex", self, n, _MODULES[__name__], __debug__)


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
