---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# 0x0ffc 资金流向日数据

## 作用

按一只或多只证券读取 7709 主站最近的日资金流向记录，每只证券通常返回最近 5 个交易日。它不是实时推送接口，也不负责计算 K 线复权。

标准客户端默认在创建时完成普通和专用两组主站测速。资金流向与集合竞价共享 35 台专用主站的内存排名，但各自按需建立业务连接；首次请求和后续请求都不再测速。`probe_hosts=False` 跳过自动测速。

| 项目 | 内容 |
| --- | --- |
| 主要调用 | `client.money_flow.daily(code, *, include_raw=False, batch_size=75)` |
| 底层接口 | [`0x0ffc`](../COMMANDS_7709.md#cmd-0x0ffc) |
| 返回模型 | 单只代码返回 `MoneyFlowBlock`；代码列表返回 `MoneyFlowBatch` |
| 单次返回 | 每只证券通常含最近 5 条日记录 |

## 示例

```python
from eltdx import TdxClient

with TdxClient(timeout=5) as client:
    flow = client.money_flow.daily("sz000063")
    for row in flow.records:
        print(row.date, row.main_net, row.main_ratio)

    flows = client.money_flow.daily(
        ["sz000063", "sh600000"], batch_size=2
    )
    for block in flows.blocks:
        print(block.full_code, block.records)
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `code` | 单只证券代码或代码列表，支持 `sz000063`、`000063` 这类写法；没有市场前缀时按代码段补全 |
| `include_raw` | 是否保留协议原始响应；默认 `False`。无论是否开启，记录级 `raw` 辅助字段都会保留 |
| `batch_size` | 传入代码列表时的最大并发数，默认 `75`；实际并发数不会超过连接池容量 |

## 返回字段

### `MoneyFlowBlock`

这是“一只股票的结果外壳”。传入单只代码时返回一个 `MoneyFlowBlock`，里面的 `records` 才是每天的资金流向数据；通常有最近 5 个交易日。它不是另一种资金指标。

| 字段 | 含义 |
| --- | --- |
| `exchange` | 市场前缀：`sz`、`sh` 或 `bj` |
| `market_id` | 主站市场编号 |
| `code` | 六位证券代码 |
| `full_code` | `exchange + code` 的完整代码 |
| `records` | 按主站顺序排列的 `MoneyFlowDaily` 元组 |
| `count` | `records` 条数 |

### `MoneyFlowBatch`

传入多个代码时返回这个批量结果。`blocks` 按输入代码顺序保存每只证券的 `MoneyFlowBlock`，`count` 是所有证券的日记录总数；证券数量可用 `len(blocks)` 获取。

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
| `main_buy_net` | 主买净额：按主动买入/卖出方向汇总的四组分档净额 |
| `main_buy_ratio` | 主买净额占比，百分数值 |
| `main_buy_super_large_net` | 主买口径下的超大单净额 |
| `main_buy_large_net` | 主买口径下的大单净额 |
| `main_buy_medium_net` | 主买口径下的中单净额 |
| `main_buy_small_net` | 主买口径下的小单净额 |
| `main_super_large_net` | 主力净额口径下的超大单净额 |
| `main_large_net` | 主力净额口径下的大单净额 |
| `main_medium_net` | 主力净额口径下的中单净额 |
| `main_small_net` | 主力净额口径下的小单净额 |

其中，`main_super_large_net`、`main_large_net`、`main_medium_net`、`main_small_net` 四项组成 `main_net`；带 `main_buy_` 前缀的四项组成 `main_buy_net`，两者是不同统计口径。

### `buckets` 怎么看

协议把 16 个值按 8 组买卖对返回，每组是两个数（买入、卖出）。当前已确认 `buckets[0] - buckets[1]` 与 `buckets[4] - buckets[5]` 用于主力净额；其余位置先保留原始值，不在 SDK 中强行起业务名称：

| 索引 | 当前含义 | 状态 |
| ---: | --- | --- |
| `0`, `1` | 主力净额计算中的第一组买入 / 卖出值 | 已确认参与 `main_net` |
| `2`, `3` | 主买净额计算中的第一组买入 / 卖出值 | 已确认参与 `main_buy_net` |
| `4`, `5` | 主力净额计算中的第二组买入 / 卖出值 | 已确认参与 `main_net` |
| `6`, `7` | 主买净额计算中的第二组买入 / 卖出值 | 已确认参与 `main_buy_net` |
| `8`, `9` | 第五组买入 / 卖出值 | 保留原始值，业务名称未确认 |
| `10`, `11` | 主买净额计算中的第三组买入 / 卖出值 | 已确认参与 `main_buy_net` |
| `12`, `13` | 第七组买入 / 卖出值 | 保留原始值，业务名称未确认 |
| `14`, `15` | 主买净额计算中的第四组买入 / 卖出值 | 已确认参与 `main_buy_net` |

因此不要把 `buckets` 的单个数字直接当作“超大单金额”或“买入金额”。SDK 已提供主买、主力及两套四档净额字段；`raw` 和 `record_hex` 仍保留用于进一步核对。

服务端页面使用的主力计算口径为：

```text
main_net = (bucket[0] - bucket[1] + bucket[4] - bucket[5]) / 50000 * total_amount
main_ratio = (bucket[0] - bucket[1] + bucket[4] - bucket[5]) / 500
main_buy_net = (bucket[2] - bucket[3] + bucket[6] - bucket[7] + bucket[10] - bucket[11] + bucket[14] - bucket[15]) / 50000 * total_amount
main_buy_ratio = (bucket[2] - bucket[3] + bucket[6] - bucket[7] + bucket[10] - bucket[11] + bucket[14] - bucket[15]) / 500
main_buy_super_large_net = (bucket[2] - bucket[3]) / 50000 * total_amount
main_buy_large_net = (bucket[6] - bucket[7]) / 50000 * total_amount
main_buy_medium_net = (bucket[10] - bucket[11]) / 50000 * total_amount
main_buy_small_net = (bucket[14] - bucket[15]) / 50000 * total_amount
main_super_large_net = (bucket[0] - bucket[1]) / 50000 * total_amount
main_large_net = (bucket[4] - bucket[5]) / 50000 * total_amount
main_medium_net = (bucket[8] - bucket[9]) / 50000 * total_amount
main_small_net = (bucket[12] - bucket[13]) / 50000 * total_amount
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
          "record_hex": "...",
          "main_buy_net": 507015330.7392,
          "main_buy_ratio": 18.946,
          "main_buy_super_large_net": 444662025.5232,
          "main_buy_large_net": 59516631.2448,
          "main_buy_medium_net": 3907116.9792,
          "main_buy_small_net": -1070443.008,
          "main_super_large_net": 400292162.8416,
          "main_large_net": -27563907.456,
          "main_medium_net": -168380685.1584,
          "main_small_net": -204294048.0768
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
