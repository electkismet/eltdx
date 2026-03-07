# API Validation Review

This document records the places where `docs/API_VALIDATION_SUMMARY.md` is currently too optimistic or semantically inaccurate when compared with the real exported data in `artifacts/live_validation_20260307_042853`.

Update on the current branch:

- `get_count()` / `get_codes*()` wording has been corrected to "code-table" semantics
- `get_stock_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()` have been tightened from the earlier loose partitioning rules
- the sections below remain useful as the historical root-cause review for why those fixes were needed

## 1. `get_count(exchange)` is not a stock count

Current summary issue:

- the current wording can easily be read as "market stock count"

Why this is inaccurate:

- `counts.json` reports `sh=26779`, `sz=22736`, `bj=297`
- `codes_all_sh.json` starts with many index-like entries such as `sh999999 上证指数`, `sh999998 Ａ股指数`, `sh999997 Ｂ股指数`
- `codes_all_sz.json` starts with category-like entries such as `sz395001 主板Ａ股`, `sz395011 封闭基金`, `sz395013 ＥＴＦｓ`, `sz395032 债券回购`
- `codes_all_bj.json` now comes from the BSE fallback and contains listed BJ securities only

Correct interpretation:

- `get_count("sh")` / `get_count("sz")` currently mean **TDX code-table entry count**
- `get_count("bj")` currently means **BJ fallback code count**
- these three values are not a clean cross-market "stock count" comparison

## 2. `get_codes()` / `get_codes_all()` are not pure stock lists

Current summary issue:

- the current wording sounds too close to "security master" or "stock code table"

Why this is inaccurate:

- real exported rows include indexes, board categories, funds, REITs, repos, and other non-stock entries
- examples from `codes_page_sz_start0_limit20.json`:
  - `sz395001 主板Ａ股`
  - `sz395011 封闭基金`
  - `sz395013 ＥＴＦｓ`
  - `sz395015 REITS`
  - `sz395032 债券回购`

Correct interpretation:

- these APIs currently expose the **underlying TDX code-table entries**
- entries are mixed and should not be treated as "all stocks"

## 3. `get_stock_codes_all()` is currently semantically wrong

Current summary issue:

- it is described as "all stock full codes"

Why this is inaccurate:

- `stock_codes_all.txt` begins with clearly non-stock entries:
  - `sh999998 Ａ股指数`
  - `sh999997 Ｂ股指数`
  - `sh888880 新标准券`
  - `sh880001 总市值`
  - `sh880002 流通市值`
- these are not normal stock instruments

Root cause:

- current implementation classifies "stock" as `not ETF and not index`
- that leaves many bonds, repos, synthetic indicators, board aggregates, and other codes incorrectly falling into the stock bucket

Correct interpretation:

- `get_stock_codes_all()` currently returns **residual non-ETF and non-index code-table entries**
- it should not yet be presented as a true stock universe

## 4. `get_etf_codes_all()` is currently semantically wrong

Current summary issue:

- it is described as "all ETF full codes"

Why this is inaccurate:

- `etf_codes_all.txt` starts with many non-ETF bond-like entries:
  - `sh152010 PR梧州01`
  - `sh152011 18京投09`
  - `sh152021 PR易盛德`
  - `sh152063 PR南充债`
  - `sh152115 PR息烽债`
- these are not normal ETF instruments

Root cause:

- current ETF logic is prefix-only and too broad
- see `src/eltdx/protocol/unit.py`

Correct interpretation:

- `get_etf_codes_all()` currently returns **prefix-matched ETF-like codes**, not a rigorously filtered ETF universe

## 5. `get_index_codes_all()` is only partially correct

Current summary issue:

- it is described as if it returns the full index set, and even uses `bj899050` as an example shape

Why this is inaccurate:

- `index_codes_all.txt` has `0` BJ index entries in the real export
- it also misses obvious SH entries such as:
  - `sh999998 Ａ股指数`
  - `sh999997 Ｂ股指数`
  - `sh880001 总市值`
- those were incorrectly pushed into `stock_codes_all.txt`

Root cause:

- the current index rule is too narrow and only recognizes a subset of index patterns

Correct interpretation:

- `get_index_codes_all()` currently returns a **partial index subset**, not a complete index universe

## 6. The current stock/ETF/index partition is mechanically exhaustive, not semantically accurate

Hard evidence:

- `stock_codes_all.txt = 40850`
- `etf_codes_all.txt = 8407`
- `index_codes_all.txt = 555`
- total = `40850 + 8407 + 555 = 49812`
- `49812` exactly equals `26779 + 22736 + 297`

Why this matters:

- this shows the current three methods partitioned the entire code-table universe into three buckets
- but the raw universe contains many mixed instrument types and category rows
- so the buckets are currently **implementation buckets**, not trustworthy business buckets

## Practical correction

The current summary should be read this way:

- market count APIs: runnable, but counts are code-table counts, not stock counts
- code-table APIs: runnable, but return mixed TDX code entries
- stock / ETF / index helper APIs: runnable, but current classification semantics are not yet trustworthy enough to describe as real market universes

## Recommended next fixes

1. revise `docs/API_VALIDATION_SUMMARY.md` wording for count and code-table APIs
2. mark `get_stock_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()` as "current heuristic classification"
3. if desired, redesign classification rules from the real exchange product taxonomy instead of prefix-only shortcuts
