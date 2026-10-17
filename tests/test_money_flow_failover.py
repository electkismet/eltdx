"""资金流空数据故障转移（#22 第一步）的离线测试。

全部离线：用假 transport / 假 factory 驱动，不建立任何 socket。
核心要验的三件事：
  1. 第一台空、第二台有数据 → 返回数据，并记住那台主机；
  2. **所有候选都空 → 返回空而不抛错**（新股/真无数据不能被误报成故障）；
  3. 重试次数有上界（一批空票只探测一轮，不是每只票都重探）。
"""

from __future__ import annotations

from datetime import date

import pytest

from eltdx.api import money_flow as api_module
from eltdx.api.money_flow import MoneyFlowApi
from eltdx.exceptions import TransportError
from eltdx.models import MoneyFlowBatch, MoneyFlowBlock, MoneyFlowDaily
from eltdx.protocol.constants import TYPE_MONEY_FLOW

HOSTS = ("h1:7709", "h2:7709", "h3:7709")


def _record(total: float = 1.0) -> MoneyFlowDaily:
    return MoneyFlowDaily(
        date_raw=20261012,
        date=date(2026, 10, 12),
        total_amount=total,
        buckets=(1, 2, 3, 4),
        main_net=total,
        main_ratio=1.0,
        raw=(0,),
    )


def _block() -> MoneyFlowBlock:
    return MoneyFlowBlock(exchange="sz", market_id=0, code="000001", records=(_record(),))


def _one_block() -> MoneyFlowBatch:
    return MoneyFlowBatch((_block(),))


EMPTY = MoneyFlowBatch(())


class _FakeTransport:
    def __init__(self, host: str | None, behaviour) -> None:
        self.host = host
        self._behaviour = behaviour
        self.calls: list[str] = []
        self.closed = False

    def execute(self, command: int, payload: dict):
        assert command == TYPE_MONEY_FLOW
        code = payload["code"]
        self.calls.append(code)
        behaviour = self._behaviour
        if isinstance(behaviour, Exception):
            raise behaviour
        if callable(behaviour):
            return behaviour(code)
        return behaviour

    def close(self) -> None:
        self.closed = True

    @property
    def pool_size(self) -> int:
        return 1

    def __repr__(self) -> str:  # pragma: no cover - 仅用于失败信息
        return f"<_FakeTransport {self.host!r} calls={self.calls!r}>"


class _Factory:
    """记录每次建池。

    ``attempts`` 记录**请求过的**主机（含建池就失败的那些）；``created`` 只记录
    真正建出来的池。注意：只要提供了 factory，``_active_transport()`` 就会用它，
    传进 ``MoneyFlowApi`` 的那个主 transport 只在没有 factory 时才生效。
    """

    def __init__(self, per_host: dict[str, object], primary=None) -> None:
        self.per_host = per_host
        self.primary = primary if primary is not None else EMPTY
        self.created: list[_FakeTransport] = []
        self.attempts: list[str] = []

    def __call__(self, host: str | None = None):
        if host is not None:
            self.attempts.append(host)
        behaviour = self.primary if host is None else self.per_host[host]
        if isinstance(behaviour, Exception):
            raise behaviour
        transport = _FakeTransport(host, behaviour)
        self.created.append(transport)
        return transport


def _raising(exc: Exception):
    def _inner(code: str):
        raise exc

    return _inner


def _api(factory: _Factory, primary=None) -> MoneyFlowApi:
    if primary is not None:
        factory.primary = primary
    return MoneyFlowApi(
        _FakeTransport(None, EMPTY),
        transport_factory=factory,
        candidate_hosts=HOSTS,
    )


# ── 主路径 ─────────────────────────────────────────────────────────────────


def test_non_empty_response_is_returned_unchanged_without_failover() -> None:
    factory = _Factory({}, primary=_one_block())
    api = _api(factory)
    result = api.daily("sz000001")
    assert isinstance(result, MoneyFlowBlock)
    assert result.count == 1
    # 有数据就不该去开任何单主机池
    assert factory.attempts == []


def test_first_host_empty_second_has_data() -> None:
    factory = _Factory({"h1:7709": EMPTY, "h2:7709": _one_block(), "h3:7709": EMPTY})
    api = _api(factory)

    result = api.daily("sz000001")

    assert isinstance(result, MoneyFlowBlock)
    assert result.count == 1
    # 按候选顺序试：h1 空 → h2 有数据就停，不该再试 h3
    assert factory.attempts == ["h1:7709", "h2:7709"]


def test_all_hosts_empty_returns_empty_instead_of_raising() -> None:
    """关键语义：真无数据时所有主机都空，必须返回空，不能抛错。"""

    factory = _Factory({host: EMPTY for host in HOSTS})
    api = _api(factory)

    result = api.daily("sz000001")

    assert isinstance(result, MoneyFlowBatch)
    assert result.blocks == ()
    assert factory.attempts == list(HOSTS)


def test_recovered_host_is_reused_on_the_next_call() -> None:
    factory = _Factory({"h1:7709": EMPTY, "h2:7709": _one_block(), "h3:7709": EMPTY})
    api = _api(factory)

    api.daily("sz000001")
    assert factory.attempts == ["h1:7709", "h2:7709"]
    created_after_first = len(factory.created)

    result = api.daily("sz000002")

    # 第二次直接用已认下来的那台池，不再重建、也不再重探
    assert isinstance(result, MoneyFlowBlock)
    assert factory.attempts == ["h1:7709", "h2:7709"]
    assert len(factory.created) == created_after_first
    assert api._preferred_transport.calls == ["sz000001", "sz000002"]


def test_transport_error_on_one_host_is_skipped() -> None:
    factory = _Factory(
        {
            "h1:7709": _raising(TransportError("boom")),
            "h2:7709": _one_block(),
            "h3:7709": EMPTY,
        }
    )
    api = _api(factory)

    result = api.daily("sz000001")

    assert isinstance(result, MoneyFlowBlock)
    assert factory.attempts == ["h1:7709", "h2:7709"]
    # 报错的那台池必须被关掉，不能泄漏
    failed = [t for t in factory.created if t.host == "h1:7709"]
    assert len(failed) == 1 and failed[0].closed is True


def test_factory_failure_on_one_host_is_skipped() -> None:
    factory = _Factory(
        {
            "h1:7709": TransportError("connect refused"),
            "h2:7709": _one_block(),
            "h3:7709": EMPTY,
        }
    )
    api = _api(factory)

    result = api.daily("sz000001")

    assert isinstance(result, MoneyFlowBlock)
    # h1 在**建池**阶段就失败，所以只有 h2 真的被建出来
    assert factory.attempts == ["h1:7709", "h2:7709"]
    assert [t.host for t in factory.created if t.host is not None] == ["h2:7709"]


# ── 不该触发故障转移的情况 ──────────────────────────────────────────────────


def test_unknown_response_shape_does_not_trigger_failover() -> None:
    """形状不是批量响应（例如假 transport 回了个 dict）时不是「空数据」。"""

    factory = _Factory({host: EMPTY for host in HOSTS}, primary=lambda code: {"code": code})
    api = MoneyFlowApi(
        _FakeTransport(None, EMPTY), transport_factory=factory, candidate_hosts=HOSTS
    )

    result = api.daily("sz000001")

    assert result == {"code": "sz000001"}
    assert factory.attempts == []


def test_client_without_factory_never_fails_over() -> None:
    """非连接池客户端（没有 factory / 候选主机）保持原行为。"""

    primary = _FakeTransport(None, EMPTY)
    api = MoneyFlowApi(primary)

    result = api.daily("sz000001")

    assert isinstance(result, MoneyFlowBatch)
    assert result.blocks == ()
    assert primary.calls == ["sz000001"]


def test_client_without_candidate_hosts_never_fails_over() -> None:
    factory = _Factory({host: EMPTY for host in HOSTS})
    primary = _FakeTransport(None, EMPTY)
    api = MoneyFlowApi(primary, transport_factory=factory, candidate_hosts=())

    api.daily("sz000001")

    assert factory.attempts == []


# ── 批量：探测次数必须有上界 ────────────────────────────────────────────────


def test_batch_probe_count_is_bounded() -> None:
    """整批探测次数有上界，不会开 N×主机数 条连接。

    封顶单位是**次数**而不是「整批只探一轮」：每只票最多试 ``_MAX_PROBES_PER_CODE`` 台，
    整批预算为 ``min(票数, _BUDGET_CODE_CAP) × _MAX_PROBES_PER_CODE``。
    """

    codes = [f"sz{i:06d}" for i in range(1, 41)]
    factory = _Factory({host: EMPTY for host in HOSTS})
    api = _api(factory)

    result = api.daily(codes)

    assert isinstance(result, MoneyFlowBatch)
    assert result.blocks == ()
    budget = min(len(codes), api_module._BUDGET_CODE_CAP) * api_module._MAX_PROBES_PER_CODE
    assert len(factory.attempts) <= budget


def test_batch_does_not_let_first_empty_code_block_later_codes() -> None:
    """关键回归：首只票在所有主机都空，**不能**连带让后面的票跳过备用主机。

    「某台主机对这只票返回空」是标级事实。早期实现用整批共享的 ``settled`` 布尔量，
    首票探完即认定「全主机都空」，于是 ``sz000002`` 连 h2 都不试就被丢掉了。
    """

    factory = _Factory(
        {
            "h1:7709": lambda code: EMPTY,
            # h2 只对 sz000002 有数据
            "h2:7709": lambda code: _one_block() if code == "sz000002" else EMPTY,
            "h3:7709": lambda code: EMPTY,
        }
    )
    api = _api(factory)

    result = api.daily(["sz000001", "sz000002"])

    assert len(result.blocks) == 1
    assert result.blocks[0].code == "000001"


def test_batch_probe_start_rotates_across_codes() -> None:
    """每只票的探测起点轮转，避免整批票都去死磕 candidate_hosts[0]。"""

    factory = _Factory({host: EMPTY for host in HOSTS})
    api = _api(factory)

    api.daily(["sz000001", "sz000002", "sz000003", "sz000004"])

    # 三台候选主机都应被覆盖到，且没有任何一台独占全部探测
    assert set(factory.attempts) == set(HOSTS)
    assert max(factory.attempts.count(host) for host in HOSTS) < len(factory.attempts)


def test_extra_host_is_still_reached_by_later_codes() -> None:
    """前两台都空时，轮转保证后面的票仍能试到第三台。"""

    factory = _Factory(
        {
            "h1:7709": lambda code: EMPTY,
            "h2:7709": lambda code: EMPTY,
            "h3:7709": lambda code: _one_block() if code == "sz000005" else EMPTY,
        }
    )
    api = _api(factory)

    result = api.daily(["sz000005", "sz000006"])

    assert len(result.blocks) == 1


def test_batch_recovers_and_queries_remaining_codes_on_good_host() -> None:
    factory = _Factory({"h1:7709": EMPTY, "h2:7709": _one_block(), "h3:7709": EMPTY})
    api = _api(factory)

    result = api.daily(["sz000001", "sz000002", "sz000003"])

    assert isinstance(result, MoneyFlowBatch)
    assert len(result.blocks) == 3
    # 只建了 h1（空）与 h2（有数据）两个池，其余票直接落在 h2 上
    assert factory.attempts == ["h1:7709", "h2:7709"]


# ── close() 生命周期 ───────────────────────────────────────────────────────


def test_close_releases_dedicated_and_preferred_transports() -> None:
    factory = _Factory({"h1:7709": EMPTY, "h2:7709": _one_block(), "h3:7709": EMPTY})
    api = _api(factory)
    api.daily("sz000001")
    preferred = api._preferred_transport
    assert preferred is not None

    api.close()

    assert preferred.closed is True
    assert api._preferred_transport is None
    assert api._preferred_host is None


def test_new_backup_connection_is_closed_when_preferred_already_adopted() -> None:
    """已有首选连接时，新探测到的备用连接既不被接管也必须被关闭。

    回归：``_adopt()`` 早退时若不把所有权交回调用方，这条连接会一路泄漏到
    ``close()`` 之后（``close()`` 只关 ``_preferred_transport``，不认识它）。
    """

    factory = _Factory(
        {
            # h1 只对 sz000001 有数据 → 第一次调用认下 h1
            "h1:7709": lambda code: _one_block() if code == "sz000001" else EMPTY,
            # h2 只对 sz000002 有数据 → 第二次调用需新开 h2
            "h2:7709": lambda code: _one_block() if code == "sz000002" else EMPTY,
            "h3:7709": EMPTY,
        }
    )
    api = _api(factory)
    api.daily("sz000001")
    assert api._preferred_host == "h1:7709"

    result = api.daily("sz000002")
    assert isinstance(result, MoneyFlowBlock)
    api.close()

    leaked = [t.host for t in factory.created if not t.closed]
    assert leaked == []


def test_single_code_path_does_not_leak_probe_connections() -> None:
    """单只全空标的：逐台探测建出来的池都必须关闭。"""

    factory = _Factory({host: EMPTY for host in HOSTS})
    api = _api(factory)

    api.daily("sz000001")
    api.close()

    probe_transports = [t for t in factory.created if t.host is not None]
    assert probe_transports  # 确实建过备用池
    assert [t.host for t in probe_transports if not t.closed] == []


def test_empty_result_is_a_single_block_when_one_block_present() -> None:
    factory = _Factory({"h1:7709": EMPTY, "h2:7709": _one_block(), "h3:7709": EMPTY})
    api = _api(factory)
    # 单块应被拆出来（与既有的 _single 行为一致）
    assert isinstance(api.daily("sz000001"), MoneyFlowBlock)


def test_multi_block_response_is_returned_as_batch() -> None:
    two = MoneyFlowBatch((_block(), _block()))
    factory = _Factory({}, primary=two)
    api = _api(factory)
    result = api.daily("sz000001")
    assert isinstance(result, MoneyFlowBatch)
    assert len(result.blocks) == 2


@pytest.mark.parametrize("bad", [0, -1, True, 1.5, None])
def test_batch_size_is_still_validated(bad) -> None:
    api = _api(_Factory({}))
    with pytest.raises(ValueError):
        api.daily("sz000001", batch_size=bad)  # type: ignore[arg-type]
