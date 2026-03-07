# Architecture

This document explains what each layer in `eltdx` is responsible for, which layer normal users should call directly, and why the `services` modules exist.

## Overview

`eltdx` currently has four layers:

1. `protocol`
2. `transport`
3. `client`
4. `services`

The intended rule is:

- most users call `TdxClient`
- advanced users may use `services`
- `protocol` and `transport` are internal implementation layers

## Layer 1: `protocol`

Location:

- `src/eltdx/protocol/`

Responsibility:

- build Tongdaxin request frames
- decode response frames and payloads
- convert wire values into Python models
- normalize time, exchange, code, and kline frequency values

Examples:

- `model_quote.py`
- `model_kline.py`
- `model_trade.py`
- `unit.py`

This layer should not contain user workflow logic.

## Layer 2: `transport`

Location:

- `src/eltdx/transport/connection.py`
- `src/eltdx/transport/router.py`
- `src/eltdx/transport/reader.py`
- `src/eltdx/transport/heartbeat.py`

Responsibility:

- open and close sockets
- handshake with Tongdaxin servers
- send request frames
- route responses back by `msg_id`
- keep the connection alive with heartbeats

Current split:

- `TdxConnection`: orchestration and request lifecycle
- `ResponseRouter`: pending-response registration and delivery
- `ResponseReader`: socket read loop and frame decoding callback
- `HeartbeatLoop`: periodic heartbeat sending

This layer is internal and should stay behavior-focused, not business-focused.

## Layer 3: `client`

Location:

- `src/eltdx/client.py`

Responsibility:

- provide the main public API surface
- expose stable method names and signatures
- batch large quote requests automatically
- manage pooled long-lived connections
- return typed response models

This is the primary entry point for users.

Examples of direct user calls:

- `get_quote()`
- `get_kline()`
- `get_trades()`
- `get_codes_all()`
- `get_call_auction()`

Design rule:

- if a capability is a general market-data operation, it belongs on `TdxClient`

## Layer 4: `services`

Location:

- `src/eltdx/services/codes.py`
- `src/eltdx/services/workday.py`
- `src/eltdx/services/gbbq.py`

Responsibility:

- provide reusable higher-level helpers
- organize common lookup and cache behavior
- avoid repeating the same client-side composition logic

Important design rule:

- `services` are not example scripts
- `services` are not the main public API freeze target
- `services` must provide general helper logic, not narrow business workflows

In practice, this means a service may help with:

- lookup
- filtering
- classification
- date navigation
- in-memory caching
- factor derivation

But a service should not silently assume a narrow workflow such as:

- only fetch 20 or 50 symbols
- only process one exchange by default for business reasons
- automatically run a full-market pipeline without the caller explicitly asking for it

## Why `services` exist

Without `services`, users can still do everything directly with `TdxClient`.

For example, a user can always:

- fetch all codes from `sh`, `sz`, and `bj`
- filter stock or ETF codes manually
- compute workday traversal manually
- combine `gbbq`, `xdxr`, and kline calls manually

`services` exist so that common helper logic becomes reusable code rather than repeated ad hoc scripts.

## Current service philosophy

The current service modules are intentionally implemented as “minimal usable services”.

That means:

- they are real code, not placeholders
- they are tested and callable today
- they stay in-memory only for now
- they do not yet include SQLite, cron, or incremental persistence logic

This lets the project move toward the target architecture without prematurely adding heavy database and scheduler complexity.

## Current services

### `CodesService`

Purpose:

- load and cache code tables in memory
- classify securities into stock, ETF, and index groups
- support lookup by full code

Current abilities:

- `refresh()`
- `all()`
- `by_exchange()`
- `get()`
- `get_name()`
- `stocks()` / `etfs()` / `indexes()`

What it is good for:

- building code lookup tables once
- preparing symbol sets before batch quote requests
- avoiding repeated manual classification logic

### `WorkdayService`

Purpose:

- normalize and navigate trading dates
- optionally build an in-memory trade calendar from index day kline data

Current abilities:

- `refresh()`
- `is_workday()`
- `today_is_workday()`
- `range()`
- `iter_days()`
- `next_workday()`
- `previous_workday()`

What it is good for:

- iterating over historical trading dates
- building backfill and replay loops
- avoiding repeated date/calendar boilerplate

### `GbbqService`

Purpose:

- cache `gbbq` data by code
- derive `xdxr`, equity change data, turnover, and adjustment factors from cached code-level data

Current abilities:

- `refresh(code)`
- `clear(code=None)`
- `get_gbbq()`
- `items()`
- `get_xdxr()`
- `get_equity_changes()`
- `get_equity()`
- `get_turnover()`
- `get_factors()`
- `get_adjusted_kline()`
- `get_adjusted_kline_all()`

What it is good for:

- repeated per-symbol factor and equity queries
- reusing one code’s `gbbq` response across several helper operations

## Which layer should users choose?

### Choose `TdxClient` when

- you want market data directly
- you are building normal scripts, crawlers, or research tools
- you want the stable public interface

This should be the default choice.

### Choose `services` when

- you repeatedly need helper logic around codes, workdays, or `gbbq`
- you are building a higher-level application on top of `eltdx`
- you want reusable in-memory indexing helpers

This is an advanced layer, not the required layer.

## What `services` should not become

The project should avoid turning `services` into hard-coded business workflows.

Bad examples:

- “fetch the first 30 stocks and quote them” as a core service behavior
- “always load only a fixed market subset” without caller control
- “implicitly run a full-market data job” when a user only asked for a lookup helper

Good examples:

- explicit refresh methods
- code lookup tables
- trading-day navigation
- per-code `gbbq` and factor caching

## Where the project is now

As of the current refactor state:

- `protocol` is stable and tested
- `transport` has been split into smaller components
- `TdxClient` is the main public interface and already suitable for first-release usage
- `services` are no longer placeholders; they are small but real reusable helpers

The next future step for `services` would be persistence and incremental update logic, but those are intentionally outside the current first-release scope.
