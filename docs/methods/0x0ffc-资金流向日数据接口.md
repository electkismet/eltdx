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

这是“一只股票的结果外壳”。一次调用返回一个 `MoneyFlowBlock`，里面的 `records` 才是每天的资金流向数据；通常有最近 5 个交易日。它不是另一种资金指标。

| 字段 | 含义 |
| --- | --- |
| `exchange` | 市场前缀：`sz`、`sh` 或 `bj` |
| `market_id` | 主站市场编号 |
| `code` | 六位证券代码 |
| `full_code` | `exchange + code` 的完整代码 |
| `records` | 按主站顺序排列的 `MoneyFlowDaily` 元组 |
| `count` | `records` 条数 |

### `MoneyFlowDaily`

这是“一天的资金流向记录”。例如 `flow.records[0]` 就是最新交易日，`flow.records[1]` 是上一条记录。

| 字段 | 含义 |
| --- | --- |
| `date_raw` / `date` | 记录日期原值 / `datetime.date` |
| `total_amount` | 当日总成交金额，单位元 |
| `buckets` | 主站返回的 16 个原始分档值，按索引原样保留；不是直接的元金额 |
| `main_net` | 主力净额：超大单和大单买入金额减卖出金额 |
| `main_ratio` | 主力净额占比，百分数值，例如 `13.928` 表示 `13.928%` |
| `raw` | 21 个原始 `uint32` 字段，便于继续核对尚未命名的字段 |
| `record_hex` | 88 字节日记录的原始十六进制 |

### `buckets` 怎么看

协议把 16 个值按 8 组买卖对返回，每组是两个数（买入、卖出）。当前已确认 `buckets[0] - buckets[1]` 与 `buckets[4] - buckets[5]` 用于主力净额；其余位置先保留原始值，不在 SDK 中强行起业务名称：

| 索引 | 当前含义 | 状态 |
| ---: | --- | --- |
| `0`, `1` | 主力净额计算中的第一组买入 / 卖出值 | 已确认参与 `main_net` |
| `2`, `3` | 第二组买入 / 卖出值 | 保留原始值，业务名称未确认 |
| `4`, `5` | 主力净额计算中的第二组买入 / 卖出值 | 已确认参与 `main_net` |
| `6`, `7` | 第四组买入 / 卖出值 | 保留原始值，业务名称未确认 |
| `8`..`15` | 其余四组买入 / 卖出值 | 保留原始值，业务名称未确认 |

因此不要把 `buckets` 的单个数字直接当作“超大单金额”或“买入金额”。目前能直接使用的业务字段是 `main_net` 和 `main_ratio`；需要继续研究其他分档含义时，再结合 `raw` 和 `record_hex` 做对照。

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
