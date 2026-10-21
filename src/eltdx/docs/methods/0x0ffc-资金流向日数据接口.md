---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# 0x0ffc 资金流向日数据

## 作用

查询一只或多只证券的日资金流向，每只证券通常返回最近 5 个交易日。

标准客户端默认不进行主站测速。资金流向与集合竞价使用独立的专用主站池，并按需建立业务连接；需要根据延迟选择节点时，显式设置 `probe_hosts=True`。

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
| `batch_size` | 传入代码列表时的最大同时查询数；默认跟随连接池容量，需要批量处理时可显式调整 |

## 返回字段

### `MoneyFlowBlock`

传入单只代码时返回 `MoneyFlowBlock`，其 `records` 包含该证券每天的资金流向数据，通常有最近 5 个交易日。

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
| `raw` | 21 个原始 `uint32` 字段 |
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

`buckets` 包含 16 个原始分档值，按 8 组买卖对排列，每组两个数（买入、卖出）。这些值需要按下方公式换算为金额：

| 索引 | 含义 | 用途 |
| ---: | --- | --- |
| `0`, `1` | 主力净额计算中的第一组买入 / 卖出值 | 用于 `main_net` |
| `2`, `3` | 主买净额计算中的第一组买入 / 卖出值 | 用于 `main_buy_net` |
| `4`, `5` | 主力净额计算中的第二组买入 / 卖出值 | 用于 `main_net` |
| `6`, `7` | 主买净额计算中的第二组买入 / 卖出值 | 用于 `main_buy_net` |
| `8`, `9` | 第五组买入 / 卖出值 | 保留原始值，业务名称未确认 |
| `10`, `11` | 主买净额计算中的第三组买入 / 卖出值 | 用于 `main_buy_net` |
| `12`, `13` | 第七组买入 / 卖出值 | 保留原始值，业务名称未确认 |
| `14`, `15` | 主买净额计算中的第四组买入 / 卖出值 | 用于 `main_buy_net` |

查询资金净额时，可直接使用 `main_net`、`main_buy_net` 及两套四档净额字段，单位均为元。`buckets` 保留原始分档值。

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

`buckets` 是记录中 8 个小端 `uint32` 打包得到的 16 个小端 `uint16` 值，`raw` 保留记录中的原始字段。

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

## 客户端「资金流向」用的就是这个接口（逐行对齐验证）

通达信 PC 客户端「公司资料 → 资金动向 → 资金流向」以及资金流向浮窗（`webs/cfg/hszjlx.html`、`hkzjlx.html`、`uszjlx.html`）都走同一条链路：

```text
页面 JS  f_callpc({Param: "99|1#600900"})
  → window.TDXQuery({request: {Method: "ASK_OneZJLX", Param: "99|<market>#<code>"}})
  → 客户端原生层 → 7709 0x0ffc（msg_id 里编 channel 0x2d / route 0x7e）
```

页面拿到的字符串里，每条记录以逗号分隔：`[0]` 日期、`[1]` 成交额、`[2..17]` 就是本页的 16 个分档值；页面按下面这套公式渲染，与本页字段一一对应：

| 客户端显示 | 页面公式 | 本页字段 |
| --- | --- | --- |
| 主力净额（按买卖单） | `(o2-o3 + o6-o7) / 5e4 * o1` | `records[i].main_net` |
| 主力占比 | 同一分子 `/ 500` | `records[i].main_ratio` |
| 主买净额（按主动性） | `(o4-o5 + o8-o9 + o12-o13 + o16-o17) / 5e4 * o1` | `records[i].main_buy_net` |
| 主买占比 | 同一分子 `/ 500` | `records[i].main_buy_ratio` |
| 超大单净额 | `(o2-o3) / 5e4 * o1` | `records[i].main_super_large_net` |
| 大单净额 | `(o6-o7) / 5e4 * o1` | `records[i].main_large_net` |

实测对照（`sh600900`，2026-10-13 收盘后同一份数据，客户端界面 vs `client.money_flow.daily("sh600900")`）：

| 日期 | 客户端 主力净额 / 占比 / 超大单净额 | 本接口解出 |
| --- | --- | --- |
| 2026-10-13 | -1.49亿 / -5.88% / -1.63亿 | -149,341,409 / -5.88% / -162,848,656 |
| 2026-10-12 | -2.74亿 / -8.66% / -1.93亿 | -274,218,437 / -8.66% / -192,504,003 |
| 2026-10-09 | -2.54亿 / -6.56% / -1.77亿 | -254,389,827 / -6.56% / -177,041,045 |
| 2026-10-08 | 1.08亿 / 3.30% / 1223.33万 | 108,072,548 / 3.30% / 12,233,394 |
| 2026-09-30 | 8978.44万 / 4.63% / -1846.11万 | 89,784,391 / 4.63% / -18,461,067 |

### 能不能取到更长的历史？（实测不能）

页面每 30 秒会再发一次 `Param: "99|1#600900|0"`（只刷新当日）。为了确认「多传一点参数就能拿更长的历史」，直接改了 38 字节请求体里的可疑字节做对照（同一条连接、同一标的）：

| 改动 | 返回 |
| --- | --- |
| 原样（尾部全 0） | 5 条：`20261013 20261012 20261009 20261008 20260930` |
| `byte[1]=1`（或 `20`） | **1 条**：只有当日 |
| `byte[8]=1`（或 `20`） | **1 条**：只有当日 |
| `byte[9]` / `byte[36]` / `byte[37]` 改 1 / 20 / 0x14 | 仍是 5 条，无变化 |

结论：`byte[1]`（或 `byte[8]`）是「只取当日」的开关，**服务端把序列固定为最近 5 个交易日**，没有可用的「起始日 / 条数」字段。客户端界面上超过 5 行的历史，来自客户端自己累积的本地数据或另一条通道，不是这个接口一次返回的。

（`daily()` 的上限就是最近 5 个交易日；需要 3 / 5 / 10 / 20 / 60 日的窗口请用本页末「历史资金流向（`client.money_flow.history`，7615 TQLEX）」一节的 `client.money_flow.history(days=...)`。）

### 与 MAC `0x1218 Stock_ZJLX` 的关系

MAC 协议（`eltdx.mac.MacClient.get_capital_flow`）的 `0x1218` 也返回当日主力净额：同一时刻实测 `sh600900` 为 `-149,341,504`，与本接口的 `-149,341,409` 同为 **-1.49亿**（差异来自两条通道各自的服务端算法，不是解析问题）。所以当日数可以互为旁证，但**16 个分档值与比例只有 `0x0ffc` 有**。

## 历史资金流向（`client.money_flow.history`，7615 TQLEX）

3 / 5 / 10 / 20 / 60 日窗口的历史资金流向走 **7615 TQLEX HTTP 网关**，与 `daily()` 的
7709 通道互相独立：

| 项目 | 内容 |
| --- | --- |
| 主要调用 | `client.money_flow.history(code, *, days=60, start=0, count=None, include_raw=False, batch_size=75)` |
| 底层接口 | `POST http://static.tdx.com.cn:7615/TQLEX?Entry=CWServ.tdxf10_gg_sj`，body `{"Params": ["zjlx_sj", code, str(days), code]}`（第 4 位是逗号分隔代码串，单只时就是自己） |
| 返回模型 | 单只代码返回 `FundFlowRangeSeries`；代码列表返回 `dict[str, FundFlowRangeSeries]` |

| 参数 | 含义 |
| --- | --- |
| `days` | 服务端窗口步长，只接受 `3` / `5` / `10` / `20` / `60`，其它值抛 `ValueError` |
| `start` / `count` | 客户端切片：`start` 为距最新偏移，`count=None` 表示 `start` 之后的全部；`count` 非空时上限 `800`（与 K 线同一页大小） |
| `include_raw` | 是否保留 `raw_payload`（网关响应重新序列化后的紧凑 JSON 字节） |
| `batch_size` | 传入代码列表时的最大并发；实际并发不超过连接池容量 |

`FundFlowRangeSeries` 字段：`code`（本次查询的完整代码）、`days`、`codes`（实际发给服务端的
六位代码）、`rows`（原样保留 `N00x` 列，服务端顺序即最新在前）、`raw_payload`、`count`。

`rows` 的列语义（与网关返回列一一对应，不做二次解读）：

| 列 | 含义 |
| --- | --- |
| `N001` | 日期 |
| `N002` | 代码 |
| `N003` | 名称 |
| `N004` | 主力净额 |
| `N005` | 超大单净买入 |
| `N006` | 大单净买入 |
| `N007` | 主买净额 |
| `N009` | 上市日期 |

```python
from eltdx import TdxClient

with TdxClient(timeout=5) as client:
    series = client.money_flow.history("sz000001", days=60)
    print(series.days, series.count)
    for row in series.rows[:5]:
        print(row["N001"], row["N004"], row["N005"], row["N006"])

    latest_five = client.money_flow.history("sz000001", days=60, start=0, count=5)
    both = client.money_flow.history(["sz000001", "sh600000"], days=20)
    print(sorted(both))
```

!!! note "需要 7615 通道"
    `history()` 通过 `TdxClient` 使用（`client.f10` 已挂载），或给 `MoneyFlowApi` 传
    `f10_factory`；两者都没有时抛 `RuntimeError`。它不再碰 7709，也不做空数据换台。

### 已放弃：`0x052d` category 22（历史记录）

同一条 `0x052d` 命令还有 **category 22** 这个周期，**已放弃，改用上面的 `history(days=…)` 7615 通道**。
它**不是**客户端「资金流向」用的通道：客户端页面（`webs/cfg/hszjlx.html` 等）走的是 `ASK_OneZJLX`，
也就是本页的 `0x0ffc`（见上文），而 category 22 在所有可达主站上恒返回空（2 字节 `0000`）。
保留这段是为了把 `0x052d` 的命令边界写清楚，并保留原始字节供将来复核：

| | `0x0ffc`（`daily()`） | 7615 `CWServ.tdxf10_gg_sj`（`history()`） | `0x052d` category 22（已放弃） |
| --- | --- | --- | --- |
| 覆盖范围 | 通常最近 5 个交易日 | `days` 取 3 / 5 / 10 / 20 / 60 | 按 `start` / `count` 请求，上限与 K 线一致（800） |
| 记录内容 | 16 个分档值 + 主力/主买两套净额与占比 | 按上面的 `N00x` 列原样返回 | 四档买入金额 + 四档卖出金额 |
| 主站可用性 | 多数主站有数据 | 网关直接有数据 | **目前一律返回空** |

!!! warning "免费主站对 category 22 返回空（实测）"
    在 18 台可达主站（普通池 + 资金流向专用池）上，category 22 一律返回 **2 字节 `0000`**，而同一连接上的 `category=4`（日线）正常有数据。已排除请求构造差异：本包的 42 字节帧与 easy_tdx 的 28 字节帧、`parameter` 取 `0`/`1`、`count` 取 `5`/`10`/`100`、`msg_id` 走普通序号或 `0x2d/0x7e` 路由，结果都是空。这是**服务端行为**，所以 `history()` 不再使用这条通道，也不再为空数据换台。

!!! note "记录布局尚无线上样本"
    非空记录的布局（9 字节前缀 + `u16` 条数 + 每条 36 字节：`u32` 日期 + 8 个通达信自定义浮点金额）继承自 easy_tdx 的参考实现，目前没有主站返回过记录，**未经线上验证**。既然 category 22 已放弃，`history()` 不再依赖这个布局；`FundFlowHistorySeries` / `FundFlowHistoryRecord` 与 Rust 侧的 `history_fund_flow` DTO 仍保留，供将来复核这条通道的原始字节使用。

## 相关接口

| 需求 | 使用 |
| --- | --- |
| 日资金流向、主力净额和分档值 | `client.money_flow.daily(code)` |
| 历史资金流向（3/5/10/20/60 日） | `client.money_flow.history(code, days=...)` |
| 不复权、前复权或后复权 K 线 | `client.bars.get(code, adjust=...)` |
| 本地不复权 K 线自行复权 | `client.corporate.adjustment_factors(...)` |
