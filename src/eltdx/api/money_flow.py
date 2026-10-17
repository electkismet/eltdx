"""7709 daily money-flow API."""

from __future__ import annotations

from collections.abc import Sequence
from concurrent.futures import ThreadPoolExecutor
from threading import Lock

from eltdx.exceptions import TransportError
from eltdx.models import MoneyFlowBatch, MoneyFlowBlock
from eltdx.protocol.constants import TYPE_MONEY_FLOW
from eltdx.protocol.unit import normalize_code

from .base import ApiBase


#: 单只标的一次故障转移最多试用的备用主机数（避免新股在几十台主机上全试一遍）。
_MAX_PROBES_PER_CODE = 3
#: 整批预算按前 N 只标的计算：更靠后的票即使全空也不再重试，天然给连接数封顶。
_BUDGET_CODE_CAP = 8


class _EmptyFailover:
    """单次 ``daily()`` 调用内共享的空数据故障转移状态。

    存在的意义是**给重试次数封顶**：一批 75 只票若每只都在几十台候选主机上逐台重试，
    会开 ``75 × 候选主机数`` 条连接。

    !!! warning "封顶口径：按「次数」封顶，不按「结论」封顶"
        「某台主机对这只票返回空」是**标级**事实，推不出「它对别的票也返回空」。
        早期实现用一个 ``settled`` 布尔量表达「本轮已探完」，于是首只股票（新股、
        停牌）在所有候选主机上都空时，整批剩下的股票**连备用主机都不再试** —— 直接漏数据。
        改成按主机记账也不对：h2 对票 A 空不代表对票 B 空，第二只票照样会被跳过。

        现在的封顶是**次数预算**：

        * 每只票最多试 ``_MAX_PROBES_PER_CODE`` 台（``probes_left`` 为全局预算，
          ``cursor`` 让每只票的起点轮转，避免所有票都去死磕同一台主机）；
        * 预算耗尽后不再重试，直接返回空响应。

        这样「重试次数有上界」和「不因个别标的为空而漏掉其它标的的数据」同时成立。

    ``lock`` 保证多线程批量查询时只有一个线程真正去逐台探测。
    """

    __slots__ = ("lock", "transport", "host", "probes_left", "cursor")

    def __init__(self, budget: int) -> None:
        self.lock = Lock()
        self.transport = None
        self.host: str | None = None
        self.probes_left = budget
        self.cursor = 0

    def take(self) -> bool:
        """原子地取一个探测配额；返回 ``False`` 表示预算已耗尽。"""

        if self.probes_left <= 0:
            return False
        self.probes_left -= 1
        return True


class MoneyFlowApi(ApiBase):
    """Read the latest daily money-flow records.

    Args:
        transport: 主 transport。
        transport_factory: 可选，无参调用时返回资金流直连池；带 ``host`` 调用时返回
            只连该主机的临时池（用于空数据故障转移）。
        candidate_hosts: 空数据时按顺序尝试的主机（已按延迟排好序）。
    """

    def __init__(
        self,
        transport,
        *,
        transport_factory=None,
        candidate_hosts: Sequence[str] = (),
    ) -> None:
        super().__init__(transport)
        self._transport_factory = transport_factory
        self._candidate_hosts = tuple(candidate_hosts)
        self._dedicated_transport = None
        self._preferred_transport = None
        self._preferred_host: str | None = None
        self._transport_lock = Lock()

    def _active_transport(self):
        # 一旦某台主机被证明「真有数据」，后续查询直接用它，避免每次空返回都重探一轮。
        if self._preferred_transport is not None:
            return self._preferred_transport
        if self._transport_factory is None:
            return self._transport
        with self._transport_lock:
            if self._dedicated_transport is None:
                self._dedicated_transport = self._transport_factory()
            return self._dedicated_transport

    def close(self) -> None:
        with self._transport_lock:
            if self._dedicated_transport is not None:
                self._dedicated_transport.close()
                self._dedicated_transport = None
            if self._preferred_transport is not None:
                self._preferred_transport.close()
                self._preferred_transport = None
            self._preferred_host = None

    def daily(
        self,
        code: str | Sequence[str],
        *,
        include_raw: bool = False,
        batch_size: int = 75,
    ):
        _validate_batch_size(batch_size)
        if isinstance(code, str):
            failover = _EmptyFailover(_MAX_PROBES_PER_CODE)
            return self._single(code, include_raw=include_raw, failover=failover)

        codes = [normalize_code(item) for item in code]
        if not codes:
            raise ValueError("codes must not be empty")
        workers = _batch_workers(self._transport, len(codes), batch_size)
        blocks: list[MoneyFlowBlock] = []
        # 整批共享一个**次数预算**：重试总数有上界，但绝不因为「第一只票全空」就
        # 认定后续票也不用试备用主机（那是标级事实，不能跨票推广）。
        budget = min(len(codes), _BUDGET_CODE_CAP) * _MAX_PROBES_PER_CODE
        failover = _EmptyFailover(budget)
        with ThreadPoolExecutor(max_workers=workers) as executor:
            for start in range(0, len(codes), workers):
                chunk = codes[start : start + workers]
                for response in executor.map(
                    lambda item: self._single(
                        item, include_raw=include_raw, failover=failover
                    ),
                    chunk,
                ):
                    if isinstance(response, MoneyFlowBlock):
                        blocks.append(response)
                    elif isinstance(response, MoneyFlowBatch):
                        blocks.extend(response.blocks)
                    else:
                        return response
        return MoneyFlowBatch(tuple(blocks))

    def _single(self, code: str, *, include_raw: bool, failover: _EmptyFailover | None = None):
        response = self._active_transport().execute(
            TYPE_MONEY_FLOW, {"code": code, "include_raw": include_raw}
        )
        blocks = getattr(response, "blocks", None)
        if blocks is None:
            # 形状不是我们认识的批量响应 —— 这**不是**「空数据」，原样返回、不触发故障转移。
            return response
        if len(blocks) == 1:
            return blocks[0]
        if blocks:
            return response
        # 真正的空数据：可能是这台服务器没有该数据（easy_tdx 同款韧性行为）。
        recovered = self._recover_empty(code, include_raw=include_raw, failover=failover)
        return response if recovered is None else recovered

    def _recover_empty(
        self, code: str, *, include_raw: bool, failover: _EmptyFailover | None
    ) -> object | None:
        """空数据时逐台主机重试，返回首个有数据的结果；全都为空则返回 ``None``。

        **关键语义（与 easy_tdx 一致）**：新股 / 真无数据的标的在所有主机上都是空的，
        此时必须**返回空而不是抛错**，否则会把「本来就没数据」误报成故障。所以本方法
        试完可用主机后返回 ``None``，由调用方原样返回那个空响应。

        封顶由 :class:`_EmptyFailover` 的**次数预算**保证（不是「整批只探一轮」的结论），
        因此首只标的全空不会连带让后面的标的跳过备用主机。候选主机按 ``cursor`` 轮转
        起点，避免整批票都去死磕 ``candidate_hosts[0]``。
        """

        if failover is None or not self._candidate_hosts or self._transport_factory is None:
            return None
        candidates = self._candidate_hosts
        total = len(candidates)
        with failover.lock:
            if failover.transport is not None:
                # 本批已经找到过「真有数据」的主机，直接复用，不再开新连接。
                return self._query(failover.transport, code, include_raw)
            start = failover.cursor % total
            for step in range(min(_MAX_PROBES_PER_CODE, total)):
                if not failover.take():
                    return None
                host = candidates[(start + step) % total]
                if host == self._preferred_host:
                    continue  # 已知它对当前数据也空，跳过
                transport = self._open_host_transport(host)
                if transport is None:
                    continue
                try:
                    result = self._query(transport, code, include_raw)
                except TransportError:
                    # 单台主机连不上/超时不该拖垮整个查询，换下一台。
                    _close_quietly(transport)
                    continue
                if result is not None:
                    adopted = self._adopt(host, transport)
                    if adopted:
                        failover.transport = transport
                        failover.host = host
                    else:
                        # 已有首选连接（并发下被别的线程抢先认下）：查询结果照常返回，
                        # 但这条连接不归本对象所有，必须由我们关掉，否则泄漏。
                        _close_quietly(transport)
                    return result
                # 这台对当前标的是空的：换下一台。**不**把它当作「所有主机都空」的结论，
                # 下一只标的重试时仍会从轮转后的起点继续试。
                _close_quietly(transport)
            # 起点推进一格，让下一只标的换一批主机试。
            failover.cursor = (start + 1) % total
            return None

    def _adopt(self, host: str, transport) -> bool:
        """记住这台有数据的主机（仅进程内，不落盘；``close()`` 会释放）。

        Returns:
            ``True`` 表示本对象接管了 ``transport``（调用方不要再关它）；
            ``False`` 表示已有首选连接、本连接**未被接管**，调用方必须自行关闭。
        """

        with self._transport_lock:
            if self._preferred_transport is not None:
                return False
            self._preferred_transport = transport
            self._preferred_host = host
            return True

    def _open_host_transport(self, host: str):
        try:
            return self._transport_factory(host)
        except TransportError:
            return None

    @staticmethod
    def _query(transport, code: str, include_raw: bool) -> object | None:
        """在指定 transport 上查一次；空数据返回 ``None``（供调用方判断换台）。"""

        response = transport.execute(
            TYPE_MONEY_FLOW, {"code": code, "include_raw": include_raw}
        )
        blocks = getattr(response, "blocks", None)
        if blocks is None:
            return None
        if len(blocks) == 1:
            return blocks[0]
        return response if blocks else None


def _close_quietly(transport) -> None:
    try:
        transport.close()
    except Exception:  # noqa: BLE001 - 尽力而为的清理，不能掩盖真正的查询结果
        pass


def _validate_batch_size(value: int) -> None:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ValueError("batch_size must be a positive integer")


def _batch_workers(transport, code_count: int, batch_size: int) -> int:
    capacity = getattr(transport, "pool_size", 1)
    if isinstance(capacity, bool) or not isinstance(capacity, int) or capacity <= 0:
        capacity = 1
    return min(code_count, batch_size, capacity)
