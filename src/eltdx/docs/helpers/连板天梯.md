---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# 实时涨停梯队

`client.helpers.limit_ladder()` 是基于 7709 实时行情和本地统计资源组合出来的**当前交易日实时筛选器**，不是 7615 的历史 F10 列表接口。

它把当前封板状态、历史连板统计、竞价数据、盘口封单和基础资料合并成 `ShortlineIndicator`，再筛选出已封板或触板股票，按连板高度和封单金额排序。

## 调用

| 项目 | 内容 |
| --- | --- |
| 调用 | `client.helpers.limit_ladder(codes=None, include_touched=False, count=None)` |
| 返回 | `LimitLadderTable` |
| 数据范围 | 当前交易日；集合竞价完成（通常 09:25）后可用 |
| 默认代码 | `codes=None` 时读取代码表并扫描全部 A 股 |

## 数据来源与处理流程

```text
codes=None
  ↓
client.codes.all_a_shares()                 # 全部 A 股代码
  ↓
shortline_indicators(full_codes)
  ├─ 0x06b9 / zhb.zip：历史涨停、连板和统计窗口
  ├─ 0x054c：实时行情快照
  ├─ 0x0547：五档盘口，用于当前买一/封单
  ├─ 0x054b：开盘抢筹分类行情
  ├─ 最近 5 个完整交易日 K 线
  ├─ 代码表和财务快照
  └─ 交易日上下文与日期对齐
  ↓
根据实时状态计算 limit_status、ladder_level、seal_amount 等字段
  ↓
过滤 sealed（以及可选的 touched）并排序
```

默认过滤和排序规则等价于：

```python
rows = [
    row for row in table.rows
    if row.limit_status == "sealed"
    or (include_touched and row.limit_status == "touched")
]
rows.sort(key=lambda row: (
    -(row.ladder_level or 0),
    -(row.seal_amount or 0.0),
    row.full_code,
))
```

## 输入参数

| 参数 | 是否必填 | 说明 |
| --- | --- | --- |
| `codes` | 否 | 股票代码或代码列表；不传时扫描全部 A 股。传列表可显著减少请求量 |
| `include_touched` | 否 | 是否包含触板但未封板的股票，默认 `False` |
| `count` | 否 | 最多返回条数；`None` 返回全部，必须为正整数 |

## 返回结构

`LimitLadderTable` 有以下字段：

| 字段 | 含义 |
| --- | --- |
| `trade_date` | 当前行情对应的目标交易日 |
| `rows` | `ShortlineIndicator` 行元组 |
| `count` | `len(rows)` |

### 行模型的核心字段

| 字段 | 含义 | 适用条件 |
| --- | --- | --- |
| `full_code` / `exchange` / `code` | 完整代码、市场前缀、六位代码 | 始终 |
| `limit_status` | `sealed` 已封板、`touched` 触板未封、`none` 未触板、`unknown` 未知 | 始终 |
| `ladder_level` | 当前连续封板高度 | 仅 `sealed` 时有值 |
| `limit_board_text` | “几天几板”文本，例如 `7天5板` | 统计资源可对齐时 |
| `seal_amount` | 当前买一金额；封板时可视作封单额 | 盘口可用时 |
| `last_price` / `change_pct` | 最新价 / 当前涨跌幅 | 实时快照 |
| `open_price` / `open_change_pct` | 开盘价 / 开盘涨幅 | 09:25 数据可用时 |
| `open_amount` / `open_volume_hand` | 开盘成交额 / 成交量 | 09:25 数据可用时 |
| `opening_rush` | 开盘抢筹值 | `0x054b` 提供时 |
| `prev_amount` / `prev_seal_amount` | 昨日成交额 / 昨日封单额 | 统计资源对齐时 |
| `open_prev_amount_ratio` / `open_prev_seal_ratio` | 开盘昨比 / 开盘昨封比 | 分母有效时 |
| `auction_prev_volume_ratio` | 今日竞价量相对昨日竞价量倍数 | 分母有效时 |
| `seal_to_float_ratio` / `seal_prev_ratio` | 封流比 / 封昨比 | 已封板且分母有效时 |
| `free_float_shares` / `free_float_market_value` | 自由流通股本 / 自由流通市值 | 统计资源和行情可用时 |
| `float_shares` / `float_market_value` | 普通流通股本 / 普通流通市值 | 财务和行情可用时 |
| `beta_60d` / `pe_ttm` | 近 60 日 Beta / 滚动市盈率 | 统计资源可用时 |
| `limit_stat_days` / `limit_up_count_in_stat_days` | 涨停统计窗口 / 窗口内涨停次数 | 统计资源可用时 |
| `limit_up_streak_days` / `year_limit_up_days` | 文件记录的连板天数 / 年内涨停天数 | 原始统计资源字段 |
| `target_trade_date` / `previous_trade_date` / `stats_date` | 目标日、上一交易日、统计文件日期 | 日期对齐元数据 |
| `alignment_status` | `same_day` 或 `previous_trading_day` 等日期对齐状态 | 日期对齐元数据 |

完整字段定义见 [短线指标](短线指标.md) 的“指标字段”章节。字段为 `None` 通常表示数据源尚未提供、统计日期无法安全对齐、分母无效，或该字段不适用于当前状态。

## 使用限制

- 集合竞价尚未完成时，`shortline_indicators()` 会抛出 `ShortlineIndicatorsNotReadyError`，因此不适合 09:25 前调用。
- `codes=None` 会扫描全部 A 股，盘中高频刷新时建议传入候选代码列表并设置 `count`。
- 统计资源在同一个客户端内按路径缓存；跨交易日或使用 `refresh_stats=True` 时会重新检查资源。
- 这是当前实时组合结果；如需读取 7615 的涨跌停明细，可使用 [`client.f10.limit_up_down_list()`](../methods/F10-涨跌停列表.md)。

## 与 7615 涨跌停明细接口的区别

| 需求 | 推荐接口 |
| --- | --- |
| 盘中实时查看当前封板、连板高度和封单 | `client.helpers.limit_ladder()` |
| 包含触板未封股票 | `client.helpers.limit_ladder(include_touched=True)` |
| 查询某个日期或日期范围的 7615 明细 | `client.f10.limit_up_down_list()` |
| 同时查看涨停、炸板、跌停和服务端原因 | `client.f10.limit_up_down_list(..., include_summary=True)` |

## 示例

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    # 推荐先传候选代码；不传 codes 会扫描全部 A 股
    table = client.helpers.limit_ladder(
        codes=["sz000001", "sh600000"],
        include_touched=True,
        count=30,
    )
    print(table.trade_date, table.count)
    for row in table.rows:
        print(
            row.full_code,
            row.limit_status,
            row.ladder_level,
            row.seal_amount,
            row.limit_board_text,
        )
```

## 真实返回样本

??? return-sample "真实返回 JSON · LimitLadderTable（1 条节选）"
    <div class="return-sample-meta">
      <div><span>目标交易日</span><code>2026-08-17</code></div>
      <div><span>返回类型</span><code>LimitLadderTable</code></div>
    </div>
    <p class="return-sample-note">真实采样；实际每行还包含上方列出的完整短线指标字段。</p>

    ```json
    {
      "trade_date": "2026-08-17",
      "rows": [
        {
          "full_code": "sz000001",
          "limit_status": "sealed",
          "ladder_level": 2,
          "limit_board_text": "2天2板",
          "seal_amount": 1234567.0,
          "last_price": 11.56,
          "change_pct": 10.0,
          "open_amount": 456789.0,
          "opening_rush": 8.2
        }
      ],
      "count": 1
    }
    ```
