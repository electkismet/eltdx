# 扩展行情：EX（7727）、MAC 与 MAC-EX

eltdx 除 A 股 7709 主站外，还提供三个**独立协议栈**，用于期货、期权、港股、外盘、外汇等非 A 股市场，以及新版通达信的 MAC 通道。它们分别是独立子包，各自管理连接与帧编解码，不经过 7709 引擎。

| 能力 | 子包 | 端口 / 帧 | 说明 |
| --- | --- | --- | --- |
| 扩展行情 | `eltdx.ex` | 7727 / 0x01 帧 | 期货、期权、港股、外盘、外汇等 |
| MAC 协议 | `eltdx.mac` | MAC 服务器 / 0x1c 帧 | 批量报价、排行、复权 K 线、板块、异动、集合竞价等 |
| MAC-EX | `eltdx.ex.MacExClient` | 7727 / MAC 风格命令 | 在扩展行情连接上使用 MAC 命令（登录 `0x2454` + goods 系列） |

命令行入口见 [CLI.md](CLI.md)（`eltdx ex` / `eltdx mac` / `eltdx macex` / `eltdx fund`）。

## 服务端能力（实测，2026-10）

免费主站的能力**并不对称**，先看清这张表能省很多时间：

| 命令 | `MAC_EX_HOSTS`（2 台，7727） | `KNOWN_EX_HOSTS`（16 台，7727） |
| --- | --- | --- |
| `get_instrument_count` | ✅ | ✅ |
| `get_instrument_info` | ✅ | ⚠️ 部分可用 |
| `get_markets` | ❌ 超时 | ❌ 超时 |
| `get_instrument_quote` / `bars` / `minute` / `txn` | ❌ 超时 | ❌ 超时或被 RST |
| `MacExClient.*`（MAC 命令） | ✅ | ❌ |
| `MacClient.*`（7709 MAC） | 不适用 | 不适用（走 `MAC_HOSTS`） |

推论：

- **只要 EX 协议主机都不给行情**，`get_instrument_quote` 这类命令在免费主站上取不到数据。
- **场外基金等品种只有走 MAC-EX 才有行情**（见下一节）。
- `get_markets()` 在所有内置主站上都超时；`ExMarket` 枚举本身就是完整市场表，
  需要市场清单时直接用枚举（`eltdx ex markets` 会自动回退到枚举）。

  > **这是服务端行为，不是 eltdx 的缺陷**（本轮已用差分实测确认）：
  > easy_tdx 1.20.7 在**同一主机**上同样超时（`TdxConnectionError: 通信错误: timed out`），
  > 而两者的 `GetExMarketsCmd` 请求帧**逐字节相同**（`01 02 48 69 00 01 02 00 02 00 f4 23`），
  > 且同一连接上 `get_instrument_count()` 在两边都正常返回 `107165`。
  > 把 `get_markets` 作为连接上的**第一条**命令发送同样超时（3 台主机均如此），
  > 所以也与命令顺序、连接状态无关——服务器就是不回应这一条命令。
- **不要用批量探测去打这些主站**：短时间大量连接/请求会触发限流，
  之后连 `get_instrument_count` 都会被 RST，需冷却约 2 分钟。调试时请用
  「单连接 + 先做一次 sanity 调用」的方式。

## 扩展行情 `eltdx.ex.ExTdxClient`

```python
from eltdx.ex import ExTdxClient

with ExTdxClient.from_best_host() as client:
    total = client.get_instrument_count()              # 全市场标的总数（无参数）
    page = client.get_instrument_info(0, 100)          # 按全局偏移分页
    quote = client.get_instrument_quote(47, "IFL0")
    quotes = client.get_instrument_quote_list(47, 4)   # 需要 category
    bars = client.get_instrument_bars(4, 47, "IFL0", count=100)   # category, market, code
    minutes = client.get_minute_time_data(47, "IFL0")
    ticks = client.get_transaction_data(47, "IFL0")
    hist = client.get_history_instrument_bars_range(47, "IFL0", 20260101, 20260131)
```

| 方法 | 签名要点 |
| --- | --- |
| `get_markets()` | 无参数（服务端普遍不支持，建议改用 `ExMarket` 枚举） |
| `get_instrument_count()` | **无参数**，返回全市场标的数 |
| `get_instrument_info(start, count=100)` | **按全局偏移分页**，不是 `(market, code)` |
| `get_instrument_quote(market, code)` | 单合约五档行情（含持仓量） |
| `get_instrument_quote_list(market, category, start=0, count=80)` | **必须给 category** |
| `get_instrument_bars(category, market, code, start=0, count=700, *, bar_time="start")` | **第一个参数是 category**；`bar_time="end"` 把分钟级 bar 对齐到右端点 |
| `get_history_instrument_bars_range(market, code, start_date, end_date, *, bar_time="start")` | 区间历史 K 线；**不支持 `bar_time="end"`**（缺周期信息，会抛 `ValueError`） |
| `get_minute_time_data(market, code)` | 当日分时 |
| `get_history_minute_time_data(market, code, date)` | 指定日期历史分时 |
| `get_transaction_data(market, code, start=0, count=1800)` | 当日逐笔 |
| `get_history_transaction_data(market, code, date, start=0, count=1800)` | 历史逐笔（港股自动翻页取全量） |

`ExMarket` 枚举描述支持的市场编号（期权、外汇、外盘、港基金、基金、德股、美股等，共 53 项）。
`KNOWN_EX_MARKETS` 只是其中一部分的**中文别名表**（11 项，不含 33/34/93 等基金市场），
它不做任何校验，不要拿它当市场白名单。默认服务器见 `eltdx.ex.KNOWN_EX_HOSTS`、
`eltdx.ex.MAC_EX_HOSTS`。

### 主机选择：并发探活 + 失败自动负载均衡

扩展行情的可用性比 7709 差得多：16 台内置主机 **TCP 全部可连**，但只有极少数会应答
业务命令。实测 `0x23f0`（商品总数）**16 台全部应答**，而 `0x23f5`（枚举）**只有 1 台应答**。
所以「按 TCP 顺序故障转移」没有意义——**连得上不等于用得了**。

`ExTdxClient` 因此改为：

1. **并发探活**：`connect()` 用真实业务命令（默认 `0x23f5` 枚举）并发探测全部候选主机，
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

!!! warning "不要自己发 `EX_SETUP_CMD` 握手帧（公开 API 已移除）"
    实测发送该握手后，服务器只回一帧恒定的 299 字节，**之后对任何命令都不再应答**，
    等于把连接打死。它原本是从 `easy_tdx` / `pytdx` 移植过来的一个**无人调用的常量**，
    此前曾被包装成公开方法 `ExTdxClient.send_setup()`；由于该 API 只会破坏连接，
    已连同 `eltdx.ex.commands.setup` 模块一并**移除**（该模块从未随任何版本发布）。
    `connect()` 从不发送它，这是正确的——扩展行情不需要握手。

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

!!! warning "取行情必须用 MAC-EX，不能用 `ExTdxClient`"
    扩展行情主机上的 `get_instrument_quote` / `get_instrument_bars` 对 `market=33`
    会超时；只有 **MAC 命令**（`MacExClient`）有数据。

```python
from eltdx.ex import MacExClient

with MacExClient.from_best_host() as c:          # 走 MAC_EX_HOSTS，端口 7727
    # 净值快照
    c.goods_quotes([(33, "016947")])
    # [MacQuoteField(market=33, code='016947', name='国泰利安中短债A',
    #                fields={'pre_close': 1.0924, 'close': 1.0924, 'vol': 0, ...})]

    # 日线净值（实测返回 700 条）
    c.goods_kline(33, "016947", count=5)

    # 枚举基金：代码 + 名称
    c.instruments(33, count=10)
    # [ExInstrumentInfo(category=8, market=33, code='000001', name='华夏成长', desc=''), ...]

    # 某市场的标的数量
    c.goods_count(33)                             # 27556
```

基金没有成交量，`vol` / `amount` 恒为 0，价格字段即**单位净值**。

!!! note "MAC 商品列表命令 0x2562 多数主站不响应"
    `goods_list()` 优先用 0x2562；实测内置主站对**任何** market 都只回 2 字节
    `0000`（`total=0`）。eltdx 会自动退回 EX 元数据（`GetExInstrumentInfoCmd`）
    实现，因此不会拿到空列表；`goods_count(market)` 同样会自动退回二分计数。
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
- **分时与逐笔可用**：`goods_tick_chart` 分时点从 **21:30** 起（服务端时间，即美股
  时段）；`goods_transaction` 返回 `time` / `price` / `vol` / `bs_flag`。
- 标的范围不限于股票：`AAA`（先行CLO债券ETF）、`AAAU`（伦敦金定盘价格）等
  ETF / 权证（`AAC+`）同在 `market=74`，靠 `instruments()` 的名称区分。

已知限制：

- `ExInstrumentInfo.desc` 带 `\x00` 填充与尾部杂字节（协议如此），只用
  `code` / `name` / `category` 即可。
- **国际指数**（`market=12`，`INTL_INDEX`）的 `DJI` / `IXIC` / `SPX` 代码直查返回空，
  正确代码格式未验证；要美股大盘指数时先 `instruments(12)` 看实际代码。
- 与场外基金一样，**取行情必须用 `MacExClient`**（`MAC_EX_HOSTS`，端口 7727），
  不要用 `ExTdxClient` 的 quote / bars 命令。


## MAC 协议 `eltdx.mac.MacClient`

```python
from eltdx.mac import MacClient, Period, Adjust

with MacClient() as client:
    quotes = client.get_stock_quotes([(0, "000001"), (1, "600000")])   # 市场编号 0=深 1=沪 2=北
    ranking = client.get_stock_quotes_list(category=..., sort_by=...)
    bars = client.get_stock_kline(0, "000001", period=Period.DAILY, adjust=Adjust.QFQ)
    minutes = client.get_stock_minute_time(0, "000001")
    ticks = client.get_stock_transaction(0, "000001")
    boards = client.get_board_list()
```

| 方法 | 说明 |
| --- | --- |
| `get_stock_quotes(stocks)` | 批量报价（字段位图，含涨停/跌停/委比/主力净流入等） |
| `get_stock_quotes_list(...)` | 分类排行报价 |
| `get_stock_kline(market, code, period, adjust, *, bar_time="start")` | K 线（全周期 + 复权）；`bar_time="end"` 把分钟级 bar 对齐到右端点 |
| `get_stock_minute_time` / `get_stock_minute_time_hist` | 单日 / 多日分时 |
| `get_tick_chart` / `get_tick_charts` / `get_chart_sampling` | 分时缩略采样 |
| `get_stock_transaction` / `get_transactions` | 逐笔 |
| `get_symbol_info` | 个股快照 |
| `get_board_list` / `get_belong_board` / `get_board_summary` / `get_board_ranking` / `get_board_change_ranking` / `get_board_members` | 板块列表、所属板块、板块汇总与排行、成分股 |
| `get_capital_flow` | 资金流 |
| `get_auction` | 集合竞价 |
| `get_unusual` | 盘口异动 |
| `get_server_info` / `get_kline_offset` / `get_file_meta` / `download_file` / `download_file_chunk` | 服务器信息、K 线偏移、文件查询与下载 |
| `get_goods_list(market, start, count)` | 商品列表（0x2562，多数主站返回空） |

`mac/adjust.py` 提供本地前复权兜底：当服务端返回负价时，用「不复权 K 线 + XDXR」重算前复权（`apply_forward_adjust` / `compute_forward_factor` / `has_bad_prices`）。

默认服务器见 `eltdx.mac.MAC_HOSTS`（端口 7709）。市场价格/涨跌停等规则由
`eltdx.mac.price_rules` 提供（市场编号同样为 0=深 1=沪）。

## MAC-EX `eltdx.ex.MacExClient`

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
| `instruments(market, start=0, count=600)` | 列出标的（`code` + `name`），走 EX 元数据，**最可靠** |
| `goods_quotes(stocks, fields=None)` | 批量报价/净值，`stocks` 为 `[(market, code)]` |
| `goods_quotes_list(market, start=0, count=100, *, sort_type=0, sort_order=0)` | 排行报价（先取标的再批量报价；排序参数保留接口） |
| `goods_kline(market, code, period=DAILY, start=0, count=800, adjust=NONE)` | K 线（支持复权） |
| `goods_count(market=None)` | 数量；`market=None` 为全市场总数 |
| `goods_list(market, start=0, count=600)` | 商品列表（0x2562，失败时自动退回元数据） |
| `goods_tick_chart(market, code, query_date=None)` | 分时 |
| `goods_transaction(market, code, query_date=None, start=0, count=2000)` | 逐笔（港股自动走 EX 协议） |
| `goods_transaction_all(market, code, query_date=None)` | 港股某日全部逐笔（自动翻页） |
| `goods_chart_sampling(market, code)` | 分时缩略采样 |

在扩展行情（7727）连接上先登录（`0x2454`），再使用 goods 系列命令；断线后 `_execute` 会自动重连并重新登录。

## 说明

- 三个子包均为**纯 Python 同步实现**，只依赖标准库；不提供 asyncio 版本。
- 返回值为 `list[dataclass]`，不是 DataFrame（与 easy_tdx 的差异之一）。
- 连接可用 `with` 管理；`MacExClient` 的默认服务器见 `eltdx.ex.MAC_EX_HOSTS`。
- `MacClient` 与 easy_tdx 的差异：缺少 `from_best_host` / `ping_all` / `ensure_connected` /
  `get_stock_kline_with_indicators`，多出 `get_stock_minute_time` /
  `get_stock_minute_time_hist` / `get_stock_transaction`。
