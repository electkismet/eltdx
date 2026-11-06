"""订阅式实时行情推送。

对齐通达信 TQ ``subscribe_hq`` / ``subscribe`` 的心智模型：订阅一组代码，
行情每推来一帧就回调一次。底层复用 eltdx 既有的
「0x0547 路由 0x29 激活 + push 队列轮询」机制，不引入新协议。

典型用法::

    def on_quote(record):
        print(record.full_code, record.last_price)

    sub = client.quotes.subscribe(["sh600000", "sz000001"], on_quote)
    sub.run()                      # 阻塞轮询，Ctrl+C 或 sub.stop() 退出
    # 或自控循环：
    #   while running:
    #       sub.run_once()          # 单轮 poll + 派发
"""

from __future__ import annotations

import math
import time
from collections.abc import Callable, Iterable, Sequence
from contextlib import ExitStack
from typing import Any

from eltdx.models.quote import QuoteRefreshPage, QuoteRefreshRecord
from eltdx.protocol.unit import normalize_code

from .base import ApiBase

__all__ = ["QuoteSubscription"]

#: 回调签名：每条刷新记录触发一次。
QuoteCallback = Callable[[QuoteRefreshRecord], Any]
#: 错误回调签名：轮询或用户回调抛错时触发。
ErrorCallback = Callable[[BaseException], Any]


def _normalize_codes(codes: str | Iterable[str]) -> tuple[str, ...]:
    """接受单个代码或代码序列，去重且保持顺序。"""

    if isinstance(codes, str):
        codes = [codes]
    seen: dict[str, None] = {}
    for code in codes:
        text = str(code).strip()
        if text:
            seen.setdefault(normalize_code(text), None)
    if not seen:
        raise ValueError("subscribe 需要至少一个证券代码")
    return tuple(seen)


class QuoteSubscription:
    """一组代码的实时行情订阅句柄。

    参数:
        api: 发起订阅的 :class:`eltdx.api.quotes.QuoteApi`。
        codes: 证券代码（``sh600000`` 形式），也用作派发白名单。
        callback: 每条刷新记录触发一次，参数为
            :class:`eltdx.models.quote.QuoteRefreshRecord`
            （含 ``full_code``、最新价、五档、成交额等）。
        timeout: 单次 ``poll_push`` 的阻塞超时（秒）。默认 0.5 ——
            既能及时响应 ``stop()``，又不会空转烧 CPU。
        filter_codes: 只派发 ``codes`` 中的记录（默认开启，防串流）。
        on_error: 轮询出错或回调抛错时的兜底；不提供则异常直接向上抛。
        reactivate_interval: 携带最新服务端游标续期的间隔，默认 7 秒。
            ``None`` 禁用自动续期。每组最多 100 只，独占原连接直到 close()。

    线程模型:
        单线程阻塞轮询。``stop()`` 设计为可从信号处理函数或其它线程安全调用。
    """

    def __init__(
        self,
        api: ApiBase,
        codes: str | Iterable[str],
        callback: QuoteCallback,
        *,
        timeout: float = 0.5,
        filter_codes: bool = True,
        on_error: ErrorCallback | None = None,
        reactivate_interval: float | None = 7.0,
    ) -> None:
        if not callable(callback):
            raise TypeError("callback 必须是可调用对象")
        if timeout is not None and timeout < 0:
            raise ValueError("timeout 必须 >= 0")
        if reactivate_interval is not None and (
            not math.isfinite(reactivate_interval) or reactivate_interval <= 0
        ):
            raise ValueError("reactivate_interval 必须 > 0")
        self._api = api
        self._codes = _normalize_codes(codes)
        self._callback = callback
        self._timeout = timeout
        self._filter_codes = filter_codes
        self._on_error = on_error
        self._reactivate_interval = reactivate_interval
        self._code_set = frozenset(self._codes)
        self._stopped = False
        self._frames = 0
        self._dispatched = 0
        self._groups = tuple(self._codes[i:i + 100] for i in range(0, len(self._codes), 100))
        self._cursors = dict.fromkeys(self._codes, 0)
        self._group_apis: list[Any] = []
        self._next_renew: list[float | None] = []
        self._pin_stack: ExitStack | None = None
        self._poll_api = api

    # ------------------------------------------------------------------ 属性

    @property
    def codes(self) -> tuple[str, ...]:
        """订阅的代码列表（去重后）。"""

        return self._codes

    @property
    def stopped(self) -> bool:
        """是否已请求停止。"""

        return self._stopped

    @property
    def frames(self) -> int:
        """已轮询到的推送帧数（含空帧）。"""

        return self._frames

    @property
    def dispatched(self) -> int:
        """已派发给回调的记录数。"""

        return self._dispatched

    # ------------------------------------------------------------------ 动作

    def activate(self):
        """主动请求当前行情页；本方法自身不派发回调。

        每组在独占连接上刷新，记录服务端游标并安排后续续期。
        """

        self._prepare_connections()
        pages = []
        try:
            for index in range(len(self._groups)):
                pages.append(self._refresh_group(index))
        except BaseException:
            self._release_connections()
            raise
        if len(pages) == 1:
            return pages[0]
        return QuoteRefreshPage(
            self._codes, tuple(record for page in pages for record in page.records), b""
        )

    def _prepare_connections(self) -> None:
        if self._group_apis:
            return
        transport = self._api._transport
        pin = getattr(transport, "pin", None)
        capacity = getattr(transport, "pool_size", 1)
        if len(self._groups) > capacity or (len(self._groups) > 1 and pin is None):
            raise ValueError(f"subscription needs at least {len(self._groups)} pool slots")
        if pin is None:
            self._group_apis = [self._api]
        else:
            # Load price metadata before reserving every pool slot; metadata
            # requests must not wait on the leases held by this subscription.
            resolver = self._api._price_resolver
            resolver = resolver() if resolver is not None else None
            if resolver is not None:
                resolver("refresh_stream", None, list(self._codes))
            stack = ExitStack()
            try:
                apis = [type(self._api)(stack.enter_context(pin())) for _ in self._groups]
            except BaseException:
                stack.close()
                raise
            self._pin_stack = stack
            self._group_apis = apis
            # One consumer owns the shared push queue for all groups. Native
            # precision metadata was populated before taking the leases.
            self._poll_api = type(self._api)(transport)
        self._next_renew = [None] * len(self._groups)

    def _remember(self, records: Sequence[Any]) -> None:
        for record in records:
            if isinstance(record, QuoteRefreshRecord) and record.full_code in self._cursors:
                code = record.full_code
                # The server cursor is a time of day, not a monotonic sequence.
                # Follow the most recently received value, including a new
                # session's lower morning time. A stale record may replay
                # some increments, but must not suppress a new session.
                self._cursors[code] = record.update_time_raw

    def _refresh_group(self, index: int):
        codes = self._groups[index]
        try:
            page = self._group_apis[index].refresh(
                list(codes), cursors={code: self._cursors[code] for code in codes}
            )
            self._remember(getattr(page, "records", ()) or ())
            return page
        finally:
            interval = self._reactivate_interval
            self._next_renew[index] = None if interval is None else time.monotonic() + interval

    def _release_connections(self) -> None:
        stack, self._pin_stack = self._pin_stack, None
        self._group_apis = []
        self._next_renew = []
        self._poll_api = self._api
        if stack is not None:
            stack.close()

    def close(self) -> None:
        """停止订阅并释放独占连接；手动 activate/run_once 用法需在结束时调用。"""
        self.stop()
        self._release_connections()

    def stop(self) -> None:
        """请求停止轮询（可从信号处理函数或其它线程调用）。"""

        self._stopped = True

    def run_once(self, *, timeout: float | None = None) -> int:
        """轮询一帧推送并派发，返回本轮派发的记录数（无帧或轮询出错返回 0）。

        续期、轮询和用户回调的异常交给 ``on_error``；未设置时向上抛出。
        """

        if self._stopped:
            return 0
        effective = self._timeout if timeout is None else timeout
        if effective is not None and effective < 0:
            raise ValueError("timeout 必须 >= 0")
        dispatched = 0
        for index, due in enumerate(self._next_renew):
            if self._stopped:
                return dispatched
            if due is not None and time.monotonic() >= due:
                try:
                    page = self._refresh_group(index)
                except Exception as exc:
                    if self._on_error is None:
                        raise
                    self._on_error(exc)
                else:
                    dispatched += self._dispatch(getattr(page, "records", ()) or ())
        if self._stopped:
            return dispatched
        due_times = [due for due in self._next_renew if due is not None]
        if due_times:
            until_renew = max(0.0, min(due_times) - time.monotonic())
            effective = until_renew if effective is None else min(effective, until_renew)
        try:
            frame = self._poll_api.poll_push(timeout=effective, parse=True)
        except Exception as exc:  # noqa: BLE001 - 与 _dispatch 一致，统一走 on_error
            if self._on_error is None:
                raise  # 未设置兜底时保留向上抛错的行为
            self._on_error(exc)
            # 轮询错误按暂时性处理：本轮结束、不计帧，run() 继续下一轮。
            return dispatched
        self._frames += 1
        if frame is None:
            return dispatched
        return dispatched + self._dispatch(getattr(frame, "records", ()) or ())

    def run(self, *, duration: float | None = None, reactivate_interval: float | None = None) -> None:
        """阻塞轮询并派发，直到 ``stop()``、达到 ``duration`` 或 KeyboardInterrupt。

        进入循环前会先 ``activate()``，激活返回的首帧快照也会派发一次，
        保证回调第一时间拿到当前状态。
        """

        if reactivate_interval is not None:
            if not math.isfinite(reactivate_interval) or reactivate_interval <= 0:
                raise ValueError("reactivate_interval 必须 > 0")
            self._reactivate_interval = reactivate_interval
        try:
            page = self.activate()
            self._dispatch(getattr(page, "records", ()) or ())
            deadline = None if duration is None else time.monotonic() + duration
            while not self._stopped:
                timeout = self._timeout
                if deadline is not None:
                    remaining = max(0.0, deadline - time.monotonic())
                    timeout = remaining if timeout is None else min(timeout, remaining)
                self.run_once(timeout=timeout)
                if deadline is not None and time.monotonic() >= deadline:
                    break
        except KeyboardInterrupt:
            self.stop()
            raise
        finally:
            self.close()

    # ------------------------------------------------------------------ 内部

    def _dispatch(self, records: Sequence[Any]) -> int:
        self._remember(records)
        count = 0
        for record in records:
            if not isinstance(record, QuoteRefreshRecord):
                continue
            if self._filter_codes and record.full_code not in self._code_set:
                continue
            self._dispatched += 1
            try:
                self._callback(record)
            except Exception as exc:  # noqa: BLE001 - 统一走 on_error 兜底
                if self._on_error is None:
                    raise
                self._on_error(exc)
            count += 1
        return count

    def __repr__(self) -> str:  # pragma: no cover - 诊断用
        return (
            f"<QuoteSubscription codes={len(self._codes)} "
            f"frames={self._frames} dispatched={self._dispatched} stopped={self._stopped}>"
        )
