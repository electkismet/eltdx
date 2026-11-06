"""Quote and quote-list API."""

from __future__ import annotations

from collections.abc import Iterable, Sequence
from typing import Any

from .base import ApiBase
from ._batch import recover_batch
from .subscription import ErrorCallback, QuoteCallback, QuoteSubscription


class QuoteApi(ApiBase):
    def get_snapshots(self, codes: str | Sequence[str], *, on_missing: str = "raise"):
        code_list = [codes] if isinstance(codes, str) else list(codes)
        return recover_batch(
            code_list,
            lambda part: self._execute_priced("snapshots", codes=part, price_codes=part),
            kind="snapshot", on_missing=on_missing,
        )

    def legacy(self, codes: str | Sequence[str]):
        """Query the old-style batch quote interface (0x053E)."""

        code_list = [codes] if isinstance(codes, str) else list(codes)
        return self._execute_priced("legacy_quotes", codes=code_list, price_codes=code_list)

    def get_depth(self, codes: str | Sequence[str]):
        """Refresh five-level quote depth through native 0x0547."""

        return self.refresh(codes, cursors={})

    def list_by_category(
        self,
        category: str | int,
        *,
        sort_by: str | int | None = None,
        start: int = 0,
        count: int = 80,
        ascending: bool = False,
    ):
        return self._execute_priced(
            "category_quotes",
            price_codes=None,
            category=category,
            sort_by=sort_by,
            start=start,
            count=count,
            ascending=ascending,
        )

    def refresh(self, codes: str | Sequence[str] | None = None, cursors: dict[str, int] | None = None):
        """Refresh up to 100 codes in one 0x0547 request."""

        code_list = None if codes is None else ([codes] if isinstance(codes, str) else list(codes))
        return self._execute_priced("refresh_stream", codes=code_list or [], cursors=dict(cursors or {}), price_codes=code_list or [])

    def poll_push(self, *, timeout: float | None = 0.0, parse: bool = False):
        poll = getattr(self._transport, "poll_push", None)
        if poll is None:
            return None
        result = poll(timeout=timeout, parse=parse)
        resolver = self._price_resolver() if self._price_resolver is not None else None
        if parse and result is not None and resolver is not None:
            return resolver("refresh_stream", result, None)
        return result

    def drain_pushes(self, *, parse: bool = False):
        drain = getattr(self._transport, "drain_pushes", None)
        if drain is None:
            return []
        result = drain(parse=parse)
        resolver = self._price_resolver() if self._price_resolver is not None else None
        if parse and resolver is not None:
            return [resolver("refresh_stream", item, None) for item in result]
        return result

    def subscribe(
        self,
        codes: str | Iterable[str],
        callback: QuoteCallback,
        *,
        timeout: float = 0.5,
        filter_codes: bool = True,
        on_error: ErrorCallback | None = None,
        reactivate_interval: float | None = 7.0,
    ) -> QuoteSubscription:
        """订阅一组代码的实时行情推送（对齐 TQ ``subscribe_hq`` 心智模型）。

        ``refresh`` 激活 + ``poll_push`` 读取服务器主动推送的组合封装，
        初始行情和后续推送记录均派发给 ``callback(record)``，``record`` 为
        :class:`eltdx.models.quote.QuoteRefreshRecord`。

        Args:
            codes: 单个代码或代码序列（``sh600000`` / ``sz000001`` 形式）。
            callback: 每条记录触发一次。
            timeout: 单次轮询的阻塞超时（秒），默认 0.5。
            filter_codes: 只派发订阅代码的记录，默认开启。
            on_error: 轮询或回调抛错时的兜底回调。
            reactivate_interval: 原连接携带最新行情游标续期，默认 7 秒；None 禁用。

        Returns:
            :class:`eltdx.api.subscription.QuoteSubscription` 句柄，
            调用 ``run()`` 阻塞轮询或 ``run_once()`` 单轮派发。
        """

        return QuoteSubscription(
            self,
            codes,
            callback,
            timeout=timeout,
            filter_codes=filter_codes,
            on_error=on_error,
            reactivate_interval=reactivate_interval,
        )

    def subscribe_hq(
        self,
        codes: str | Iterable[str],
        callback: QuoteCallback,
        **kwargs: Any,
    ) -> QuoteSubscription:
        """:meth:`subscribe` 的别名，对齐通达信 TQ 的 ``subscribe_hq`` 命名。"""

        return self.subscribe(codes, callback, **kwargs)
