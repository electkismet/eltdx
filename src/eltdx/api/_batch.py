"""Bounded recovery for malformed 7709 finance/snapshot batches."""

from __future__ import annotations

import warnings
from collections import defaultdict, deque

from eltdx.exceptions import MissingCodesError, MissingCodesWarning, ProtocolError
from eltdx.models import FinanceBatch
from eltdx.protocol.unit import normalize_code

_RECOVERY_REQUESTS = 32


def _recover_finance_native(codes, query, *, on_missing):
    """Adapt Python callbacks/exceptions to the shared native recovery tree."""
    from eltdx.transport.native import native_module

    if on_missing not in ("raise", "skip"):
        raise ValueError("on_missing must be 'raise' or 'skip'")
    native = native_module()
    responses, missing, recovered = native.recover_finance(
        codes, query, FinanceBatch, normalize_code, ProtocolError
    )
    if missing:
        if on_missing == "raise":
            raise MissingCodesError(missing)
        warnings.warn("batch skipped missing codes: " + ", ".join(missing), MissingCodesWarning, stacklevel=3)
    if not recovered:
        return responses[0]
    return native.assemble_finance(codes, responses, normalize_code, FinanceBatch, deque)


def recover_batch(codes, query, *, kind, on_missing):
    """Validate the policy, isolate missing singles, and never return corrupt rows."""
    if on_missing not in ("raise", "skip"):
        raise ValueError("on_missing must be 'raise' or 'skip'")
    missing = []
    responses = []
    remaining = _RECOVERY_REQUESTS
    recovered = False

    def fetch(part, *, initial=False):
        nonlocal remaining, recovered
        if not initial:
            if remaining == 0:
                raise ProtocolError(f"{kind} batch recovery request budget exhausted ({_RECOVERY_REQUESTS})")
            remaining -= 1
        try:
            response = query(part)
            # Preserve custom/in-memory transports' existing untyped results.
            rows = response.records if isinstance(response, FinanceBatch) else response
            if not isinstance(rows, (list, tuple)):
                responses.append(response)
                return
            if part and not rows:
                raise ProtocolError(f"empty {kind} response")
            if len(rows) != len(part):
                raise ProtocolError(f"{kind} response count mismatch: expected {len(part)}, got {len(rows)}")
            responses.append(response)
            return
        except ProtocolError as error:
            message = str(error)
            if kind == "finance":
                retryable = "finance batch" in message or message.startswith(("empty finance response", "finance response count mismatch"))
                absent = message.startswith(("finance batch missing record:", "empty finance response"))
            else:
                retryable = message.startswith(("snapshot record marker not found:", "snapshot missing requested record:", "snapshot response count mismatch:", "empty snapshot response"))
                absent = message.startswith(("snapshot missing requested record:", "empty snapshot response"))
            if not retryable or not part:
                raise
            if len(part) == 1:
                if not absent:
                    raise
                missing.append(normalize_code(part[0]))
                recovered = True
                return
            recovered = True
        middle = len(part) // 2
        fetch(part[:middle])
        fetch(part[middle:])

    fetch(codes, initial=True)
    if missing:
        if on_missing == "raise":
            raise MissingCodesError(missing)
        warnings.warn("batch skipped missing codes: " + ", ".join(missing), MissingCodesWarning, stacklevel=3)
    if not recovered:
        return responses[0]
    rows = [row for response in responses for row in (response.records if kind == "finance" else response)]
    # Reassemble in request order, including explicitly repeated input codes.
    by_code = defaultdict(deque)
    for row in rows:
        by_code[row.full_code].append(row)
    ordered = []
    for code in codes:
        bucket = by_code[normalize_code(code)]
        if bucket:
            ordered.append(bucket.popleft())
    if kind == "finance":
        # An aggregate has no single corresponding server response payload.
        return FinanceBatch(records=tuple(ordered), raw_payload=b"")
    return ordered
