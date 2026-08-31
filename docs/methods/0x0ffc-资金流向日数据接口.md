---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# 0x0ffc 资金流向日数据

## 作用

按证券代码读取 7709 主站最近的日资金流向记录，通常返回最近 5 个交易日。它不是实时推送接口，也不负责计算 K 线复权。

| 项目 | 内容 |
| --- | --- |
| 主要调用 | `client.money_flow.daily(code, *, include_raw=False)` |
| 底层接口 | [`0x0ffc`](../COMMANDS_7709.md#cmd-0x0ffc) |
| 返回模型 | `MoneyFlowBlock` |
| 单次返回 | 一个证券块，通常含最近 5 条日记录 |

## 示例

```python
from eltdx import TdxClient

with TdxClient(timeout=5) as client:
    flow = client.money_flow.daily("sz000063")
    for row in flow.records:
        print(row.date, row.main_net, row.main_ratio)
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `code` | 单只证券代码，支持 `sz000063`、`000063` 这类写法；没有市场前缀时按代码段补全 |
| `include_raw` | 是否保留协议原始响应；默认 `False`。无论是否开启，记录级 `raw` 辅助字段都会保留 |

## 返回字段

### `MoneyFlowBlock`

| 字段 | 含义 |
| --- | --- |
| `exchange` | 市场前缀：`sz`、`sh` 或 `bj` |
| `market_id` | 主站市场编号 |
| `code` | 六位证券代码 |
| `full_code` | `exchange + code` 的完整代码 |
| `records` | 按主站顺序排列的 `MoneyFlowDaily` 元组 |
| `count` | `records` 条数 |

### `MoneyFlowDaily`

| 字段 | 含义 |
| --- | --- |
| `date_raw` / `date` | 记录日期原值 / `datetime.date` |
| `total_amount` | 当日总成交金额，单位元 |
| `buckets` | 16 个分档原始数值，按主站顺序保留 |
| `main_net` | 主力净额：超大单和大单买入金额减卖出金额 |
| `main_ratio` | 主力净额占比，百分数值，例如 `13.928` 表示 `13.928%` |
| `raw` | 21 个原始 `uint32` 字段，便于继续核对尚未命名的字段 |
| `record_hex` | 88 字节日记录的原始十六进制 |

服务端页面使用的主力计算口径为：

```text
main_net = (bucket[0] - bucket[1] + bucket[4] - bucket[5]) / 50000 * total_amount
main_ratio = (bucket[0] - bucket[1] + bucket[4] - bucket[5]) / 500
```

`buckets` 是记录中 8 个小端 `uint32` 打包得到的 16 个小端 `uint16` 值；SDK 同时保留 `raw`，不把尚未确认的辅助字段误命名为业务字段。

## 真实返回样本

??? return-sample "真实返回 JSON · MoneyFlowBlock（5 条记录中的 1 条节选）"
    <div class="return-sample-meta">
      <div><span>采样标的</span><code>sz000063</code></div>
      <div><span>采样日期</span><code>2026-08-31</code></div>
      <div><span>返回类型</span><code>MoneyFlowBlock</code></div>
    </div>
    <p class="return-sample-note">真实采样（主站）；接口通常返回最近 5 个交易日，下面展示最新一条的完整解析字段。</p>

    ```json
    {
      "exchange": "sz",
      "market_id": 0,
      "code": "000063",
      "records": [
        {
          "date_raw": 20260831,
          "date": "2026-08-31",
          "total_amount": 2676107520.0,
          "buckets": [15933, 8454, 12053, 3745, 8327, 8842, 4467, 3355, 12584, 15730, 6390, 6317, 13154, 16971, 6825, 6845],
          "main_net": 372728255.3856,
          "main_ratio": 13.928,
          "raw": [1229001766, 1327465001, 555040271, 246230762, 579412104, 220205424, 1031090488, 414718200, 1111176063, 449125041, 554057277, 245444373, 579477639, 219877747, 1030893864, 413997302, 1112224610, 448600745, 1206735744, 35586615, 1830640588],
          "record_hex": "..."
        }
      ]
    }
    ```

`record_hex` 在真实返回中是完整 88 字节十六进制；文档样本用 `...` 缩略显示，`raw` 和派生字段保持真实解析值。

## 和其他行情接口的边界

| 需求 | 使用 |
| --- | --- |
| 日资金流向、主力净额和分档值 | `client.money_flow.daily(code)` |
| 不复权、前复权或后复权 K 线 | `client.bars.get(code, adjust=...)` |
| 本地不复权 K 线自行复权 | `client.corporate.adjustment_factors(...)` |

该接口只读取资金流向数据，不替代 `0x052d` K 线和 `0x000f` 股本变迁。
