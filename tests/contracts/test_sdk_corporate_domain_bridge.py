"""Authored corporate compatibility cases. Execution deferred with T01–T30."""
from __future__ import annotations

from types import SimpleNamespace

import pytest

from eltdx.api.corporate import CorporateApi
from eltdx.exceptions import MissingCodesError, MissingCodesWarning, ProtocolError
from eltdx.models import CapitalChangeBatch, CapitalChangeBlock, FinanceBatch
from eltdx.transport import PooledSocketTransport, SocketTransport


CODE = "sh" + str(600_001)
SECOND = "sh" + str(600_002)


class Transport:
    pool_size = 1

    def __init__(self, query):
        self.query = query
        self.calls = []

    def execute(self, command, payload):
        self.calls.append((command, payload.copy()))
        return self.query(command, payload)


def block(code, raw=b""):
    return CapitalChangeBlock(code[:2], 1, code[2:], 1, (), raw)


@pytest.mark.parametrize("native", [False, True])
def test_corporate_capital_prefix_keeps_duplicates_pages_and_raw(native):
    transport = Transport(lambda command, payload: block(payload["codes"][0], b"page"))
    result = CorporateApi(transport, native_services=native).capital_changes([CODE, SECOND, CODE], include_raw=True)
    assert [row.full_code for row in result.blocks] == [CODE, SECOND, CODE]
    assert result.raw_payloads == (b"page",) * 3
    assert [payload["codes"] for _, payload in transport.calls] == [[CODE, SECOND, CODE], [SECOND, CODE], [CODE]]


@pytest.mark.parametrize("native", [False, True])
@pytest.mark.parametrize("response,message", [
    (CapitalChangeBatch(()), "capital_changes returned no blocks for a non-empty request"),
    (CapitalChangeBatch((block(SECOND),)), "capital_changes returned blocks outside the requested prefix"),
    (CapitalChangeBatch((block(CODE), block(SECOND))), "capital_changes returned blocks outside the requested prefix"),
])
def test_corporate_capital_prefix_failures_keep_exact_runtime_error(native, response, message):
    with pytest.raises(RuntimeError, match=f"^{message}$"):
        CorporateApi(Transport(lambda *_: response), native_services=native).capital_changes([CODE])


@pytest.mark.parametrize("native", [False, True])
def test_corporate_capital_custom_result_is_returned_without_followup(native):
    response = object()
    transport = Transport(lambda *_: response)
    assert CorporateApi(transport, native_services=native).capital_changes([CODE, SECOND]) is response
    assert len(transport.calls) == 1


@pytest.mark.parametrize("native", [False, True])
def test_corporate_capital_chunks_keep_input_order_and_single_shape(native):
    transport = Transport(lambda command, payload: block(payload.get("code", payload.get("codes", [CODE])[0])))
    api = CorporateApi(transport, native_services=native)
    assert isinstance(api.capital_changes(CODE), CapitalChangeBlock)
    assert [row.full_code for row in api.capital_changes([CODE, SECOND, CODE], batch_size=1).blocks] == [CODE, SECOND, CODE]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
@pytest.mark.parametrize("command,invoke,payload", [
    (0x000F, lambda api: api.capital_changes(CODE), {"code": CODE, "include_raw": False}),
    (0x0010, lambda api: api.finance_batch(CODE), {"codes": [CODE], "include_raw": False}),
])
def test_corporate_native_wire_domain_is_used(transport_type, command, invoke, payload, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    calls = []
    response = object()
    monkeypatch.setattr(transport, "sdk_corporate", lambda *args: calls.append(args) or response)
    assert invoke(CorporateApi(transport, native_services=True)) is response
    assert calls == [(command, payload)]


@pytest.mark.parametrize("transport_type", [SocketTransport, PooledSocketTransport])
def test_corporate_execute_override_is_preserved(transport_type, monkeypatch):
    transport = transport_type(["127.0.0.1:7709"], heartbeat_interval=None)
    response = object()
    monkeypatch.setattr(transport, "execute", lambda *_: response)
    monkeypatch.setattr(transport, "sdk_corporate", lambda *_: pytest.fail("override bypassed"))
    assert CorporateApi(transport, native_services=True).finance_batch(CODE) is response


@pytest.mark.parametrize("native", [False, True])
def test_corporate_finance_success_keeps_original_order_and_response_identity(native):
    response = FinanceBatch((SimpleNamespace(full_code=SECOND), SimpleNamespace(full_code=CODE)), b"original")
    api = CorporateApi(Transport(lambda *_: response), native_services=native)
    assert api.finance_batch([CODE, SECOND]) is response


@pytest.mark.parametrize("native", [False, True])
def test_corporate_finance_recovery_preserves_duplicate_occurrences_and_raw_empty(native):
    serial = iter(range(10))

    def query(command, payload):
        codes = payload["codes"]
        if len(codes) > 1:
            raise ProtocolError("finance batch invalid length")
        return FinanceBatch((SimpleNamespace(full_code=codes[0], value=next(serial)),), b"single")

    transport = Transport(query)
    result = CorporateApi(transport, native_services=native).finance_batch([CODE, SECOND, CODE])
    assert [(row.full_code, row.value) for row in result.records] == [(CODE, 0), (SECOND, 1), (CODE, 2)]
    assert result.raw_payload == b""
    assert [payload["codes"] for _, payload in transport.calls] == [[CODE, SECOND, CODE], [CODE], [SECOND, CODE], [SECOND], [CODE]]


@pytest.mark.parametrize("native", [False, True])
@pytest.mark.parametrize("policy", ["raise", "skip"])
def test_corporate_finance_missing_policy_preserves_warning_or_error(native, policy):
    api = CorporateApi(Transport(lambda *_: FinanceBatch(())), native_services=native)
    if policy == "raise":
        with pytest.raises(MissingCodesError):
            api.finance_batch([CODE, CODE], on_missing=policy)
    else:
        with pytest.warns(MissingCodesWarning, match=f"batch skipped missing codes: {CODE}, {CODE}"):
            assert api.finance_batch([CODE, CODE], on_missing=policy).records == ()


@pytest.mark.parametrize("native", [False, True])
def test_corporate_finance_budget_keeps_initial_request_plus_thirty_two(native):
    transport = Transport(lambda *_: FinanceBatch(()))
    with pytest.raises(ProtocolError, match=r"^finance batch recovery request budget exhausted \(32\)$"):
        CorporateApi(transport, native_services=native).finance_batch([CODE] * 40, on_missing="skip")
    assert len(transport.calls) == 33


@pytest.mark.parametrize("native", [False, True])
def test_corporate_finance_nonrecoverable_error_preserves_instance(native):
    error = ProtocolError("unrelated framing failure")

    def query(*_):
        raise error

    transport = Transport(query)
    with pytest.raises(ProtocolError) as actual:
        CorporateApi(transport, native_services=native).finance_batch([CODE, SECOND])
    assert actual.value is error and len(transport.calls) == 1


@pytest.mark.parametrize("native", [False, True])
def test_corporate_finance_fields_keep_aliases_and_generator_consumption(native):
    response = FinanceBatch((SimpleNamespace(full_code=CODE, total_shares=12), SimpleNamespace(full_code=SECOND, total_shares=14)))
    result = CorporateApi(Transport(lambda *_: response), native_services=native).finance_batch(
        [CODE, SECOND], fields=iter(["总股本"]))
    assert result == [{"full_code": CODE, "总股本": 12}, {"full_code": SECOND}]


@pytest.mark.parametrize("native", [False, True])
def test_corporate_finance_fields_preserve_hasattr_then_getattr(native):
    class Row:
        full_code = CODE

        def __init__(self):
            self.calls = 0

        @property
        def amount(self):
            self.calls += 1
            return self.calls

    row = Row()
    api = CorporateApi(Transport(lambda *_: FinanceBatch((row,))), native_services=native)
    assert api.finance_batch(CODE, fields=["amount"]) == [{"full_code": CODE, "amount": 2}]
    assert row.calls == 2


@pytest.mark.parametrize("native", [False, True])
def test_corporate_finance_unknown_fields_keep_exact_error(native):
    api = CorporateApi(Transport(lambda *_: FinanceBatch((SimpleNamespace(full_code=CODE),))), native_services=native)
    with pytest.raises(ValueError, match="^unknown finance field: absent$"):
        api.finance_batch(CODE, fields=["absent"])
