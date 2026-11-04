# 扩展行情：EX（7727）、MAC 与 MAC-EX

eltdx 除 A 股 7709 主站外，还提供三个**独立协议栈**，用于期货、期权、港股、外盘、外汇等非 A 股市场，以及新版通达信的 MAC 通道。它们分别是独立子包，各自管理连接与帧编解码，不经过标准 7709 TdxClient 引擎。MacClient 自身默认仍连接 7709 端口。

| 能力 | 子包 | 端口 / 帧 | 说明 |
| --- | --- | --- | --- |
| 扩展行情 | `eltdx.ex` | 7727 / 0x01 帧 | 期货、期权、港股、外盘、外汇等 |
| MAC 协议 | `eltdx.mac` | MAC 服务器 / 0x1c 帧 | 批量报价、排行、复权 K 线、板块、异动、集合竞价等 |
| MAC-EX | `eltdx.ex.MacExClient` | 7727 / MAC 风格命令 | 在扩展行情连接上使用 MAC 命令（登录 `0x2454` + goods 系列） |

命令行入口见 [CLI.md](CLI.md)（`eltdx ex` / `eltdx mac` / `eltdx macex` / `eltdx fund`）。

完整的逐方法说明见 [EX / MAC / MAC-EX 单页索引](extended/README.md)：47 个业务入口、3 个连接管理页，附完整参数、分页规则、返回字段、枚举与位图。

**MAC `0x122F` 已有秒字段**：[MacClient.get_transactions](extended/mac-get_transactions.md) 支持当日/历史日查询；秒数来自服务器，但记录可聚合多笔成交。标准 `0x0FC5 / 0x0FC6` 的分钟字段属于另一套接口。

## 服务端能力（实测，2026-10）

免费主站的能力**并不对称**，先看清这张表能省很多时间：

| 命令 | `MAC_EX_HOSTS`（2 台，7727） | `KNOWN_EX_HOSTS`（16 台，7727） |
| --- | --- | --- |
| [`get_instrument_count`](extended/ex-get_instrument_count.md) | ✅ | ✅ |
| [`get_instrument_info`](extended/ex-get_instrument_info.md) | ✅ | ⚠️ 部分可用 |
| [`get_markets`](extended/ex-get_markets.md) | ❌ | ✅（完成 92B 会话握手后的 DATA 级主站） |
| [`get_instrument_quote`](extended/ex-get_instrument_quote.md) / `bars` / `minute` / `txn` | ❌ | ⚠️ 仅 DATA 级主站在完成会话握手后应答 |
| `MacExClient.*`（MAC 命令） | ✅ | ❌ |
| `MacClient.*`（7709 MAC） | 不适用 | 不适用（走 `MAC_HOSTS`） |

以下结论仅对应 2026-10 的采样主机和时间，不代表协议永久不可用：

- **只要 EX 协议主机都不给行情**，`get_instrument_quote` 这类命令在免费主站上取不到数据。
- **本次场外基金等品种的有效行情路径为 MAC-EX**（见下一节）。
- `get_markets()` 需要会话握手：DATA 级主站在收到 92B `ExSetupCmd1` 包后正常应答
  （实测 52 / 37 个市场），目录级主站仍不应答。旧版「所有主站都不回应 `0x23f4`」
  的结论来自**未发送握手包**的连接状态，见 issue #28。

  > **旧结论的成因**：pytdx `TdxExHq_API` 在 TCP 连接后固定发送一个 92B 会话包，
  > eltdx 的 `connect()` 此前只建 TCP、不发此包，于是 DATA 级主站不应答任何数据类
  > 命令（只回应 `0x23f0` 商品总数）。请求帧本身与 pytdx 逐字节相同，问题只在会话
  > 初始化。现在 `connect()` 自动发送并读走该包的应答，`get_markets()` 可作为首条调用。
- **不要用批量探测去打这些主站**：短时间大量连接/请求会触发限流，
  之后连 `get_instrument_count` 都会被 RST，需冷却约 2 分钟。调试时请用
  「单连接 + 先做一次 sanity 调用」的方式。

## 扩展行情 `eltdx.ex.ExTdxClient`

[全部方法单页与参数](extended/README.md#ex7727) · [连接与主机](extended/ex-连接与主机.md)。全部 EX 业务方法是单次调用；港股自动分页属于 MAC-EX 的成交封装。

```python
from eltdx.ex import ExTdxClient

with ExTdxClient.from_best_host() as client:
    total = client.get_instrument_count()              # 全市场标的总数（无参数）
    page = client.get_instrument_info(0, 100)          # 按全局偏移分页
    quote = client.get_instrument_quote(47, "IFL0")
    quotes = client.get_instrument_quote_list(47, 3)   # 需要 category
    bars = client.get_instrument_bars(4, 47, "IFL0", count=100)   # category, market, code
    minutes = client.get_minute_time_data(47, "IFL0")
    ticks = client.get_transaction_data(47, "IFL0")
    hist = client.get_history_instrument_bars_range(47, "IFL0", 20260101, 20260131)
```

| 方法 | 签名要点 |
| --- | --- |
| [`get_markets()`](extended/ex-get_markets.md) | 无参数；完成会话握手后是默认探活命令，`ExMarket` 枚举仍可作为离线回退 |
| [`get_instrument_count()`](extended/ex-get_instrument_count.md) | **无参数**，返回全市场标的数 |
| [`get_instrument_info(start, count=100)`](extended/ex-get_instrument_info.md) | **按全局偏移分页**，不是 `(market, code)` |
| [`get_instrument_quote(market, code)`](extended/ex-get_instrument_quote.md) | 单合约五档行情（含持仓量） |
| [`get_instrument_quote_list(market, category, start=0, count=80)`](extended/ex-get_instrument_quote_list.md) | **必须给 category** |
| [`get_instrument_bars(category, market, code, start=0, count=700, *, bar_time="start")`](extended/ex-get_instrument_bars.md) | **第一个参数是 category**；`bar_time="end"` 把分钟级 bar 对齐到右端点 |
| [`get_history_instrument_bars_range(market, code, start_date, end_date, *, bar_time="start")`](extended/ex-get_history_instrument_bars_range.md) | 区间历史 K 线；**不支持 `bar_time="end"`**（缺周期信息，会抛 `ValueError`） |
| [`get_minute_time_data(market, code)`](extended/ex-get_minute_time_data.md) | 当日分时 |
| [`get_history_minute_time_data(market, code, date)`](extended/ex-get_history_minute_time_data.md) | 指定日期历史分时 |
| [`get_transaction_data(market, code, start=0, count=1800)`](extended/ex-get_transaction_data.md) | 当日成交明细（单页，不自动翻页） |
| [`get_history_transaction_data(market, code, date, start=0, count=1800)`](extended/ex-get_history_transaction_data.md) | 历史成交明细（单页，调用方分页） |

`ExMarket` 枚举描述支持的市场编号（期权、外汇、外盘、港基金、基金、德股、美股等，共 53 项）。
`KNOWN_EX_MARKETS` 只是其中一部分的**中文别名表**（11 项，不含 33/34/93 等基金市场），
它不做任何校验，不要拿它当市场白名单。默认服务器见 `eltdx.ex.KNOWN_EX_HOSTS`、
`eltdx.ex.MAC_EX_HOSTS`。

### 主机选择：并发探活 + 失败自动负载均衡

扩展行情的可用性比 7709 差得多：16 台内置主机 **TCP 全部可连**，但 DATA 级供数主站
必须先收到 92B 会话握手包才会应答数据类命令。未握手时 `0x23f0`（商品总数）**16 台
全部应答**，而 `0x23f5`（枚举）几乎无人应答；完成握手后 `0x23f4`（市场列表）在 DATA
级主站应答、在目录级主站断开，区分度最佳。所以「按 TCP 顺序故障转移」没有意义——
**连得上不等于用得了**。

`ExTdxClient` 因此改为：

1. **并发探活**：`connect()` 完成会话握手后用真实业务命令（默认 `0x23f4` 市场列表）
   并发探测全部候选主机，
   把真正应答的按延迟排在前面。16 台并发只花一个 `probe_timeout`（默认
   `min(3.0, timeout)` 秒），而不是串行超时 16 次。单主机模式（只给 `host`，或 `hosts`
   只有一台）**不探活**，行为与旧版一致、零额外开销。
2. **命令级故障转移**：失败判定看**命令是否拿到应答**，不看 TCP。某台主机命令失败后
   立刻标记为不健康并换到次优主机，不会再回到刚失败的那台（旧实现把当前主机排在候选
   首位，导致故障转移形同虚设——这正是「查不到 016947」的根因）。
3. **失败自动负载均衡**：一次会话内失败的主机会被排除，请求自动落到其余健康主机；
   成功一次即自动恢复其健康标记；全部失败时清空标记重来一轮，避免会话被永久锁死。
   失败后会**并发重探**剩余主机，所以一次失败不会退化成 `15 × timeout` 的串行盲试。

```python
from eltdx.ex import ExTdxClient

with ExTdxClient() as c:             # 自动并发探活并选中可用主机
    print(c.host, c.healthy_hosts)   # 例：150.158.20.127 ['150.158.20.127']
    rows = c.get_instrument_info(0, 100)

client = ExTdxClient(auto_failover=False)   # 单次尝试，传输错误直接抛出
client.refresh_hosts()                      # 手动并发重探并重排优先级
```

| 参数 / 成员 | 说明 |
| --- | --- |
| `auto_failover=True` | 命令失败时换台重试；设 `False` 则单次尝试、错误直接抛出 |
| `probe_on_connect=True` | `connect()` 时并发探活选主；单主机模式自动跳过 |
| `probe_timeout=None` | 探活超时，默认 `min(3.0, timeout)` |
| `max_probe_workers=16` | 探活并发度 |
| `healthy_hosts` | 属性：已知可用且近期未失败的主机（未探活时为空 = 「没有信息」） |
| `unhealthy_hosts` | 属性：近期失败主机 → 原因 的映射 |
| `refresh_hosts(timeout=None, max_workers=None)` | 手动并发重探并重排优先级，返回探活明细 |
| `ping_all(hosts=None, port=7727, timeout=3.0)` | 静态方法：并发探活，只返回**应答业务命令**的主机 |
| `from_best_host(...)` | 并发探活后选延迟最低的**可用**主机 |

!!! note "`ping_all()` 的语义已修正"
    旧版把「TCP 能连上」当成健康，于是 16 台内置主机**全部**返回，结论毫无参考价值。
    现在只返回真正应答业务命令的主机（实测 **1/16**）。

!!! warning "探活会打主站，注意限流"
    一次 `connect()` 会并发打 16 台主机，但**每个客户端实例只探一次**（`refresh_hosts()`
    除外）。免费主站对批量探测敏感，短时间内反复新建客户端可能触发限流（表现是连
    `get_instrument_count` 都被 RST，需冷却约 2 分钟）。反复调试请复用一个实例，
    或用 `hosts=` 缩小候选范围。

### 用户可覆盖的主机列表

内置主机表会随主站增减而失效（实测 16 台里只有 1 台应答枚举命令），所以必须能在
**不发版**的前提下替换。候选来源优先级：**环境变量 > 用户配置文件 > 包内默认**。

```bash
# 方式 1：环境变量（逗号 / 分号 / 空白分隔）
export ELTDX_EX_HOSTS="203.0.113.10,198.51.100.7"
```

```json
// 方式 2：<data_dir>/ex_servers.json
{
  "schema_version": 1,
  "hosts": ["203.0.113.10", "198.51.100.7"]
}
```

```python
from eltdx.ex import ExTdxClient, ex_server_file_path, load_ex_hosts, save_ex_hosts

print(ex_server_file_path())        # 用户配置文件位置
print(load_ex_hosts())              # 当前生效的主机列表
save_ex_hosts(["203.0.113.10"])     # 原子写入用户配置（权限 0600）
```

| 项 | 说明 |
| --- | --- |
| `<data_dir>` | 由 `eltdx._paths.data_dir()` 决定：`ELTDX_DATA_DIR` > 平台标准位置（Windows `%LOCALAPPDATA%\eltdx`、macOS `~/Library/Application Support/eltdx`、Linux `$XDG_DATA_HOME/eltdx`）。**与安装包分离，重装/升级不会覆盖** |
| 主机项写法 | **裸主机名/IP**，如 `203.0.113.10`、`tdx.example.com`、`[::1]`。端口由 `ExTdxClient(port=)` 统一指定（默认 7727）——写成 `host:port` 会**明确报错**而不是静默忽略端口 |
| 配置写坏了 | 非法 JSON、缺 `hosts` 数组、条目无效都**立刻抛 `ValueError`**，不会静默回退到默认列表 —— 否则你会以为配置生效了，实际一直在用包内列表 |
| 默认顺序 | `DEFAULT_EX_HOSTS` 把实测能应答枚举命令的主机（见 `PROBE_VERIFIED_EX_HOSTS`）排在官方顺序之前，这样即使某条路径没走并发探活，首选也是能取数的那台 |

!!! note "为什么不去读通达信客户端的 `connect.cfg`"
    客户端安装目录里确实有完整主机表（`[HQHOST]` 48 台、`[DSHOST]` 16 台、`[HFHost]` 2 台等，
    其 `[DSHOST]` 与 `KNOWN_EX_HOSTS` 逐台同序一致），但各平台（Windows / Linux / macOS /
    信创）的安装路径与文件名都不固定，用户也可能根本没装客户端。因此本包只提供自己
    位置确定的配置层。

!!! warning "不提供线上主机列表拉取"
    从远端下载主机表意味着「远端可把用户流量导向任意 IP」，属供应链风险，所以**不做**。
    需要更新包内默认表时，走正常发版流程（见 `PROBE_VERIFIED_EX_HOSTS` 的实测记录）。

连接支持 `with` / `connect()` / `close()`。

!!! note "会话握手由 `connect()` 自动完成"
    `ExTdxConnection.connect()` 现在会在 TCP 建连后自动发送 pytdx `ExSetupCmd1` 同款
    92B 固定包，并**整帧读走**应答（16B 帧头 + 压缩体）。DATA 级供数主站只在收到
    此包后应答 `markets` / `quote` / `bars` 等数据类命令（issue #28）。

    旧文档里「发了握手帧之后服务器不再应答」是**客户端流错位**造成的：只发不读时，
    下一条业务命令会把 299B 握手应答当成自己的响应帧解析，静默返回空结果或报错，
    并非服务端弃连。用户无需、也不应自行发送该包；`send_setup=False` 可显式关闭，
    `mac_ex_mode=True` 的 MAC-EX 会话台另有 `0x2454` 登录流程，默认跳过握手。

## 场外基金：开放式基金、货币基金（`market=33` / `34` / `93`）

**场外基金不在 7709 交易所行情里。** 例如 `016947`（国泰利安中短债A）：

- 深市 `0x044d` 代码表 24401 条里 **`01xxxx` 号段为空**，查 7709 必然查不到；
- `normalize_code("016947")` 会把它推断成 `sz016947`，于是查询被送进错误的命名空间；
- 它在扩展行情里是一个正常标的：`market=33 (OPEN_END_FUND)`、`code='016947'`、
  `name='国泰利安中短债A'`、`category=8`。

| 市场 | 编号 | 含义 | 实测数量 |
| --- | --- | --- | --- |
| `OPEN_END_FUND` | 33 | 开放式基金 | 27556 |
| `MONETARY_FUND` | 34 | 货币基金 | 995 |
| `HK_FUND` | 49 | 香港基金 | 477 |
| `SUNSHINE_PRIVATE_FUND` | 56 | 阳光私募基金 | 2740 |
| `BROKER_COLLECTIVE_FINANCE` | 57 | 券商集合理财 | 970 |
| `BROKER_MONETARY_FINANCE` | 58 | 券商货币理财 | 4 |
| `MONEY_MARKET` | 91 | 货币市场 | 15 |
| `FUND_VALUATION` | 93 | 基金估值 | — |

!!! warning "采样主机上的基金行情使用 MAC-EX"
    扩展行情主机上的 `get_instrument_quote` / `get_instrument_bars` 对 `market=33`
    在本次采样中超时；**MAC 命令**（`MacExClient`）有数据。

```python
from eltdx.ex import MacExClient

with MacExClient.from_best_host() as c:          # 走 MAC_EX_HOSTS，端口 7727
    # 净值快照
    c.goods_quotes([(33, "016947")])
    # [MacQuoteField(market=33, code='016947', name='国泰利安中短债A',
    #                fields={'pre_close': 1.0924, 'close': 1.0924, 'vol': 0, ...})]

    # 日线净值（此处请求 5 条，实际数量以响应为准）
    c.goods_kline(33, "016947", count=5)

    # 枚举基金：代码 + 名称
    c.instruments(33, count=10)
    # [ExInstrumentInfo(category=8, market=33, code='000001', name='华夏成长', desc=''), ...]

    # 某市场的标的数量
    c.goods_count(33)                             # 27556
```

本次开放式基金样本的 `vol` / `amount` 为 0，价格字段为净值口径；不将该样本结论推广为所有基金、所有主机的固定协议规则。

!!! note "MAC 商品列表命令 0x2562 多数主站不响应"
    `goods_list()` 优先用 0x2562；实测内置主站对**任何** market 都只回 2 字节
    `0000`（`total=0`）。eltdx 会自动退回 EX 元数据（`GetExInstrumentInfoCmd`）
    实现，元数据也不可用时仍可能返回空或抛出异常；`goods_count(market)` 同样会自动退回二分计数。
    需要「代码 + 名称」成对数据时请直接用 `instruments()`——`GoodsItem` 的
    `name` 字段存的是**代码**（与 0x2562 协议语义一致），不含中文名。

## 外盘股票：美股 / 德股 / 新加坡股（`market=73/74/78`）

**外盘股票就在扩展行情里**，与场外基金一样走 MAC-EX（`MacExClient`）：
`ExTdxClient` 的 `get_instrument_quote` / `get_instrument_bars` 在免费主站上对
这些市场同样超时，只有 MAC 命令有数据。

| 市场 | 编号 | `category` | 实测标的数 | 代码样例 |
| --- | --- | --- | --- | --- |
| `US_STOCK` | 74 | 13 | **13409** | `A`（安捷伦科技）、`AA`（美国铝业）、`AAPL`（苹果）、`MSFT`（微软）、`NVDA`（英伟达）、`TSLA`（特斯拉） |
| `GE_STOCK` | 73 | 14 | 40 | `ADS`（阿迪达斯）、`AIR`（空中客车）、`BAYN`（拜耳） |
| `SG_STOCK` | 78 | 15 | 1392 | `1A0`（Katrina集团）、`1B1`（HC SURGICAL） |

```python
from eltdx.ex import MacExClient

with MacExClient.from_best_host() as c:
    n = c.goods_count(74)                                # 13409
    items = c.instruments(74, count=10)                  # 代码 + 中文名
    quotes = c.goods_quotes([(74, "AAPL"), (74, "NVDA")])  # 批量报价
    bars = c.goods_kline(74, "AAPL", count=5)            # 日 K（含 amount）
    chart = c.goods_tick_chart(74, "AAPL")               # 当日分时
    txns = c.goods_transaction(74, "AAPL")               # 逐笔
```

实测要点（2026-10-15，主机 `116.205.135.205`）：

- **枚举带中文名**：`instruments(74)` 返回 `code` + `name`（中文），美股代码即
  Ticker（`AAPL` / `MSFT` / `NVDA` / `TSLA` 直查可用）。
- **报价七字段**：`pre_close` / `open` / `high` / `low` / `close` / `vol` / `amount`
  （`goods_quotes` 默认字段），与 A 股五档不同，外盘无买卖盘口。
- **日 K 正常**：`goods_kline(74, "AAPL", count=5)` 返回 2026-10-08 ~ 10-14 五根日线，
  含 `vol` / `amount` / `float_shares`。
- **分时与成交明细可用**：`goods_tick_chart` 分时点从 **21:30** 起（服务端时间，即美股
  时段）；`goods_transaction` 返回 `time` / `price` / `vol` / `bs_flag`。
- 标的范围不限于股票：`AAA`（先行CLO债券ETF）、`AAAU`（伦敦金定盘价格）等
  ETF / 权证（`AAC+`）同在 `market=74`，靠 `instruments()` 的名称区分。

已知限制：

- `ExInstrumentInfo.desc` 带 `\x00` 填充与尾部杂字节（协议如此），只用
  `code` / `name` / `category` 即可。
- **国际指数**（`market=12`，`INTL_INDEX`）的 `DJI` / `IXIC` / `SPX` 代码直查返回空，
  正确代码格式未验证；要美股大盘指数时先 `instruments(12)` 看实际代码。
- 与场外基金一样，**本次采样中取行情使用 `MacExClient`**（`MAC_EX_HOSTS`，端口 7727），
  不要用 `ExTdxClient` 的 quote / bars 命令。


## MAC 协议 `eltdx.mac.MacClient`

[全部 23 个业务方法与别名](extended/README.md#mac7709) · [连接与主机](extended/mac-连接与主机.md)。

```python
from eltdx.mac import MacClient, Period, Adjust, Category, SortType

with MacClient() as client:
    quotes = client.get_stock_quotes([(0, "000001"), (1, "600000")])   # 此例市场编号 0=深、1=沪
    ranking = client.get_stock_quotes_list(Category.A, count=80, sort_type=SortType.CHANGE_PCT)
    bars = client.get_stock_kline(0, "000001", period=Period.DAILY, adjust=Adjust.QFQ)
    minutes = client.get_stock_minute_time(0, "000001")
    ticks = client.get_stock_transaction(0, "000001")
    boards = client.get_board_list()
```

| 方法 | 说明 |
| --- | --- |
| [`get_stock_quotes(stocks)`](extended/mac-get_stock_quotes.md) | 批量报价（字段位图，含涨停/跌停/委比/主力净流入等） |
| [`get_stock_quotes_list(...)`](extended/mac-get_stock_quotes_list.md) | 分类排行报价 |
| [`get_stock_kline(market, code, period=DAILY, start=0, count=800, times=1, adjust=NONE, *, bar_time="start")`](extended/mac-get_stock_kline.md) | K 线（全周期 + 复权）；`bar_time="end"` 把分钟级 bar 对齐到右端点 |
| [`get_stock_minute_time`](extended/mac-get_tick_chart.md) / [`get_stock_minute_time_hist`](extended/mac-get_tick_charts.md) | 单日 / 多日分时 |
| [`get_tick_chart`](extended/mac-get_tick_chart.md) / [`get_tick_charts`](extended/mac-get_tick_charts.md) / [`get_chart_sampling`](extended/mac-get_chart_sampling.md) | 单日分时 / 多日分时 / 分时缩略采样 |
| [`get_stock_transaction`](extended/mac-get_transactions.md) / [`get_transactions`](extended/mac-get_transactions.md) | 秒级聚合成交明细，0x122F，支持历史日期及自动分页 |
| [`get_symbol_info`](extended/mac-get_symbol_info.md) | 个股快照 |
| [`get_board_list`](extended/mac-get_board_list.md) / [`get_belong_board`](extended/mac-get_belong_board.md) / [`get_board_summary`](extended/mac-get_board_summary.md) / [`get_board_ranking`](extended/mac-get_board_ranking.md) / [`get_board_change_ranking`](extended/mac-get_board_change_ranking.md) / [`get_board_members`](extended/mac-get_board_members.md) | 板块列表、所属板块、板块汇总与排行、成分股 |
| [`get_capital_flow`](extended/mac-get_capital_flow.md) | 资金流 |
| [`get_auction`](extended/mac-get_auction.md) | 集合竞价 |
| [`get_unusual`](extended/mac-get_unusual.md) | 盘口异动 |
| [`get_server_info`](extended/mac-get_server_info.md) / [`get_kline_offset`](extended/mac-get_kline_offset.md) / [`get_file_meta`](extended/mac-get_file_meta.md) / [`download_file`](extended/mac-download_file.md) / [`download_file_chunk`](extended/mac-download_file_chunk.md) | 服务器信息、K 线偏移、文件查询与下载 |
| [`get_goods_list(market, start, count)`](extended/mac-get_goods_list.md) | 商品列表（0x2562，多数主站返回空） |

`mac/adjust.py` 提供本地前复权兜底：当服务端复权价格非正或非有限，且配置 xdxr_provider、重算结果有效时，用「不复权 K 线 + XDXR」重算前复权；未配置或重算无效时保留原结果（`apply_forward_adjust` / `compute_forward_factor` / `has_bad_prices`）。

默认服务器见 `eltdx.mac.MAC_HOSTS`（端口 7709）。市场价格/涨跌停等规则由
`eltdx.mac.price_rules` 提供（市场编号同样为 0=深 1=沪）。

## MAC-EX `eltdx.ex.MacExClient`

[全部 13 个业务方法](extended/README.md#mac-ex7727) · [连接与主机](extended/macex-连接与主机.md)。

```python
from eltdx.ex import MacExClient
from eltdx.mac import Period, Adjust

with MacExClient.from_best_host() as c:            # 走 MAC_EX_HOSTS，端口 7727
    # 代码 + 名称（推荐用它枚举标的）
    items = c.instruments(33, count=10)

    # 批量净值/报价
    quotes = c.goods_quotes([(33, "016947"), (33, "000001")])

    # K 线
    bars = c.goods_kline(33, "016947", period=Period.DAILY, count=100, adjust=Adjust.NONE)

    # 数量 / 商品列表 / 逐笔 / 分时
    n = c.goods_count(33)
    goods = c.goods_list(33, count=10)
    txns = c.goods_transaction(31, "00700")        # 港股走 EX 协议（当日 0x23FC / 历史 0x2406）
    chart = c.goods_tick_chart(33, "016947")
```

| 方法 | 说明 |
| --- | --- |
| [`instruments(market, start=0, count=600)`](extended/macex-instruments.md) | 列出标的（`code` + `name`），走 EX 元数据，**最可靠** |
| [`goods_quotes(stocks, fields=None)`](extended/macex-goods_quotes.md) | 批量报价/净值，`stocks` 为 `[(market, code)]` |
| [`goods_quotes_list(market, start=0, count=100, sort_type=SortType.CODE, sort_order=SortOrder.NONE)`](extended/macex-goods_quotes_list.md) | 列表报价（最多 80 项；sort_type / sort_order 当前不生效） |
| [`goods_kline(market, code, period=DAILY, start=0, count=800, adjust=NONE)`](extended/macex-goods_kline.md) | K 线（支持复权） |
| [`goods_count(market=None)`](extended/macex-goods_count.md) | 数量；`market=None` 为全市场总数 |
| [`goods_list(market, start=0, count=600)`](extended/macex-goods_list.md) | 商品列表（0x2562，返回空时自动退回元数据） |
| [`goods_tick_chart(market, code, query_date=None)`](extended/macex-goods_tick_chart.md) | 分时 |
| [`goods_transaction(market, code, query_date=None, start=0, count=2000)`](extended/macex-goods_transaction.md) | 逐笔（港股自动走 EX 协议） |
| [`goods_transaction_all(market, code, query_date=None)`](extended/macex-goods_transaction_all.md) | 港股某日成交明细（自动翻页，最多 50 页 / 90000 条） |
| [`goods_chart_sampling(market, code)`](extended/macex-goods_chart_sampling.md) | 分时缩略采样 |
| [`macro_indicators()`](extended/macex-macro_indicators.md) | 宏观指标列表，市场 38，实例缓存 |
| [`macro_quotes(codes, fields=None)`](extended/macex-macro_quotes.md) | 宏观指标最新值；完整代码或唯一裸代码 |
| [`macro_kline(code, period=MONTHLY, start=0, count=800, adjust=NONE)`](extended/macex-macro_kline.md) | 宏观报告期序列；close 为指标值 |

在扩展行情（7727）连接上先登录（`0x2454`），再使用 goods 系列命令；断线后 `_execute` 会自动重连并重新登录。

## 期货：连续合约与分时

期货走扩展行情的市场码：`28` 郑商所 / `29` 大商所 / `30` 上期所 / `47` 中金所
（`ExMarket.ZZ_FUTURES` / `DL_FUTURES` / `SH_FUTURES` / `CFFEX_FUTURES`）。
另有 `60`（`MAIN_FUTURES_CONTRACT`），实测在 MAC-EX 主站返回 0 个标的，
**主力连续请用各交易所市场内的 `L` 系列代码**。

### 连续合约命名（与常见 `L0` 惯例不同）

通达信扩展行情把连续合约命名为 `{品种}L7` / `L8` / `L9`，含义由服务端名称直接给出：

| 代码 | 名称 | 含义 |
| --- | --- | --- |
| `AUL7` | 黄金次连 | 次主力连续 |
| `AUL8` | 黄金主连 | **主力连续**（收盘价与当前主力月份合约完全一致） |
| `AUL9` | 黄金加权 | 加权指数（各月份按持仓/成交加权） |

实测（2026-10-22）：`AUL8` 与主力 `AU2612` 的 pre_close / open / high / low / close / vol
逐项相同，确认 `L8` = 主力连续；`L7`（883.90）、`L9`（883.12）与主力（881.46）不同。
覆盖品种：AU/AG/AD/AL/AO/BC/BR/BU/CU/EC/FU/HC/LU/NI/NR/OP/PB/RB/RU/SC/SN/SP/SS/WR/ZN 等，
每个品种三个连续代码（共 69 个）。

```python
from eltdx.ex import MacExClient
from eltdx.mac import Period

with MacExClient("116.205.135.205", 7727, 10.0) as c:
    # 沪金主连分时（555 个点：21:00 夜盘 → 14:59 日盘）
    chart = c.goods_tick_chart(30, "AUL8")
    print(chart.name, chart.pre_close, chart.close, len(chart.charts))
    for tick in chart.charts[:3]:
        print(tick.time, tick.price, tick.avg, tick.vol)

    # 缩略采样（图表用，约 112 点）
    points = c.goods_chart_sampling(30, "AUL8")

    # 多周期 K 线
    for period in (Period.MIN_1, Period.MIN_5, Period.MIN_60, Period.DAILY):
        bars = c.goods_kline(30, "AUL8", period)
        print(period.name, len(bars), bars[-1].datetime, bars[-1].close)
```

### 实测要点

- **分时点数随交易时段变化**：沪金 555 点（夜盘 21:00—02:30 + 日盘 09:00—15:00）、
  螺纹 345 点（夜盘 21:00—23:00 + 日盘），不是固定 240 点。
- **历史分时可取**：`goods_tick_chart(30, "AUL8", date(2026, 10, 21))` 返回 555 个
  有效价格点（`charts` 逐点完整），但**汇总字段 `open`/`high`/`low`/`close` 为 0**
  （只有 `pre_close` 有值）——取历史分时时只信 `charts`，不要用 summary。
- **`momentum` 字段不可靠**：分时点里的 `momentum` 按 float32 解析出 `3e-40` 量级的
  垃圾值，说明该 4 字节不是浮点语义（疑似整型或保留位）。目前**请忽略该字段**，
  其余 `time` / `price` / `avg` / `vol` 均正确。
- K 线各周期（1/5/15/30/60 分钟、日线）均返回 700 根，可直接用于技术指标。

## 说明

- 三个子包均为**纯 Python 同步实现**，只依赖标准库；不提供 asyncio 版本。
- 业务结果包括 dataclass、列表、字典、整数、bytes / bytearray 等，不是 DataFrame；具体类型以单页签名为准。
- 连接可用 `with` 管理；`MacExClient` 的默认服务器见 `eltdx.ex.MAC_EX_HOSTS`。
- `MacClient` 已提供 `from_best_host` / `ping_all` / `ensure_connected`；没有 `get_stock_kline_with_indicators`。分时和成交别名见各自单页。
