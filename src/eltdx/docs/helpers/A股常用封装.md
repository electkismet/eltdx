---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# A 股常用封装

这些方法都是 Python Helpers 组合，不是新的协议号。底层原始数据仍可通过对应的 `client.*` 原生入口取得。

| 功能 | 调用 | 数据来源 |
| --- | --- | --- |
| 最新股票列表 | `client.helpers.latest_stock_list()` | `0x044d` 代码表 |
| 最新 ST 列表 | `client.helpers.latest_st()` | `0x044d` 名称规则 |
| 最新停牌列表 | `client.helpers.latest_suspended()` | `0x044d` + `0x053e` 状态位 `0x20` |
| 每日股本（盘前） | `client.helpers.daily_share_capital(codes=None)` | `0x0010` 财务快照 + `0x06b9` 统计资源 |
| 每日涨跌停价 | `client.helpers.daily_price_limits(codes=None)` | `0x054c` 昨收 + 板块/ST规则 |
| 实时榜单 | `client.helpers.realtime_rank(sort_by="涨幅")` | `0x054b` 分类行情 |
| 买卖力道 | `client.helpers.buy_sell_strength(code)` | `0x051b` 分时副图 |
| 成交对比 | `client.helpers.volume_comparison(code)` | `0x051b` 分时副图 |
| 连板天梯 | `client.helpers.limit_ladder()` | 实时快照 + `0x06b9` |
| 题材强度排行 | `client.helpers.theme_strength_rank()` | 连板天梯 + F10 题材 |

```python
with TdxClient(timeout=3) as client:
    st_rows = client.helpers.latest_st()
    suspended = client.helpers.latest_suspended()
    limits = client.helpers.daily_price_limits(["sz000001", "sh600000"])
    rank = client.helpers.realtime_rank(sort_by="涨幅", count=20)
```

### 说明

- `daily_price_limits()` 是按昨收、市场板块和 ST/新股规则计算的业务结果，不冒充服务端独立的“每日价位”协议。
- `latest_suspended()` 使用旧版批量行情的公开交易状态位，只返回当前状态，不提供历史停牌日期表。
- `limit_ladder()` 默认扫描全部 A 股，数据量大时建议先传入候选代码列表。
- `theme_strength_rank()` 会读取候选股票的 F10 题材，适合盘后或小范围候选，不建议在每秒刷新循环中调用。

??? return-sample "真实返回 JSON · A 股常用封装（真实采样节选）"

    ```json
    {
      "latest_stock_list": [
        {"full_code": "sz000001", "name": "平安银行", "category": "a_share"}
      ],
      "latest_st": [],
      "latest_suspended": [],
      "daily_price_limits": {
        "full_code": "sz000001",
        "pre_close": 11.2,
        "limit_up_price": 12.32,
        "limit_down_price": 10.08,
        "limit_rule": "main_10pct"
      },
      "realtime_rank": {
        "rank": 1,
        "full_code": "sz000001",
        "change_pct": 3.21,
        "amount": 1280000000.0
      }
    }
    ```
