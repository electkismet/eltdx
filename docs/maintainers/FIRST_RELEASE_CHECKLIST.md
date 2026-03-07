# First Release Checklist

This document tracks what the first public `eltdx` release should include, what is already done, and what is intentionally deferred.

## Release target

The first public release is a Python package centered on one main entry point:

- `TdxClient`

The release target is:

- publishable to PyPI
- installable as a normal Python package
- usable directly for Tongdaxin market-data access
- stable enough for an alpha first release

The first release does **not** try to solve every later-stage data-platform problem.

## Frozen scope

The frozen first-release scope is:

- protocol parsing and request building
- connection transport and heartbeat handling
- `TdxClient` public API
- typed dataclass response models
- native Python `datetime` / `date` conversion for parsed time fields
- automatic batching for large quote requests
- pooled long-lived connections
- optional raw protocol debugging fields
- minimal reusable in-memory `services`
- package build metadata and release documentation

## Completed now

### Public API

Completed:

- `connect()` / `close()` lifecycle
- context-manager support via `__enter__` / `__exit__`
- `get_call_auction()`
- `get_quote()` with automatic batching
- `get_count()`
- `get_codes()` / `get_codes_all()`
- `get_stock_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()`
- `get_minute()`
- `get_trades()` / `get_trades_all()`
- `get_kline()` / `get_kline_all()`

Behavior already aligned with the draft:

- `pool_size=2` is the default connection strategy
- very large quote lists are split automatically instead of sending one oversized request
- short-lived usage can rely on `with TdxClient()`
- long-lived realtime usage can still keep the client open manually

### Parsing and models

Completed:

- dataclass response models for the main API
- frozen aliases such as `CodePage`, `MinuteSeries`, `TradePage`, and `KlinePage`
- native Python time conversion for parsed time fields
- timezone-aware parsed datetimes using `Asia/Shanghai`
- float + milli-int dual price representation
- optional raw hex fields for protocol debugging

### Internal architecture

Completed:

- `protocol` remains focused on frame/model encoding and decoding
- `transport` is split into `connection`, `reader`, `router`, and `heartbeat`
- `client` stays the main user-facing layer
- `services` stay reusable helpers rather than narrow example workflows

### Minimal usable services

Completed in the current lightweight form:

- `CodesService`: in-memory code lookup, filtering, and classification
- `WorkdayService`: in-memory trading-day navigation
- `GbbqService`: per-code cache and factor-related helpers

These services are intentionally usable now without forcing database or scheduler complexity into the first release.

### Packaging and docs

Completed:

- `pyproject.toml`
- `README.md`
- `CHANGELOG.md`
- architecture documentation in `docs/maintainers/ARCHITECTURE.md`
- build configuration for wheel and sdist

## Explicitly deferred

The following items are **not** blockers for the first public release:

- SQLite persistence
- scheduled sync jobs / cron / daemon loops
- incremental database update pipelines
- heavy local cache management
- release automation tied to a remote Git hosting workflow

These belong to later phases and should not delay the first package release.

## Release gate

Before tagging or uploading a first release, verify all of the following:

- public README examples still match the actual method signatures
- version in `pyproject.toml` and `src/eltdx/__about__.py` is consistent
- changelog entry matches the actual delivered scope
- tests pass locally
- live integration checks pass when enabled
- `python -m build` succeeds
- generated wheel can be imported in an isolated environment

## Recommended commands

From the project root:

```powershell
python -m pytest -q
ELTDX_RUN_LIVE=1 python -m pytest -q
python -m build
python scripts/smoke_isolated_install.py
python scripts/smoke_live_all.py
```

Recommended isolated install check:

```powershell
python -m venv .venv-release-check
.\.venv-release-check\Scripts\python -m pip install --upgrade pip
.\.venv-release-check\Scripts\python -m pip install dist\eltdx-0.1.0-py3-none-any.whl
.\.venv-release-check\Scripts\python -c "from eltdx import TdxClient; print(TdxClient)"
```

This avoids polluting the main working Python environment.

## Current status

The project is past the "protocol prototype" stage and already in a usable first-release shape for:

- market-data client usage
- typed parsing validation
- package build validation
- general helper-layer experimentation

In practical terms, the package is ready for a **first alpha release** once the release gate above is re-checked on the final code snapshot.


