# API 参考

本文档描述 `eltdx 1.0` 的对外调用方式。按方法查看参数和解析字段时，优先看 [METHOD_REFERENCE.md](METHOD_REFERENCE.md)。底层命令号、请求 payload 和响应字段以 `docs/COMMANDS_7709.md` 及协议相关文档为准。

## 总入口

真实连接 7709 主站：

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    count = client.codes.count("sz")
```

默认 `TdxClient()` 使用真实 `7709` 主站。单元测试或离线示例可以显式使用内存客户端：

```python
with TdxClient.in_memory() as client:
    request = client.codes.count("sz")
```

可以直接传主站：

```python
with TdxClient(host="116.205.183.150:7709", timeout=3) as client:
    quotes = client.get_quote(["sz000001", "sh600000"])
```

也可以使用连接池和主站测速：

```python
with TdxClient.from_hosts(pool_size=2, probe_hosts=True, timeout=3) as client:
    quotes = client.get_quote(["sz000001", "sh600000"])
```

`probe_hosts=True` 会先用 TCP connect 测一遍候选主站，把连得上的、延迟低的排在前面。默认不开测速，避免启动时等待过久。

不传 `host` / `hosts` 时，客户端会读取包内 `tdx_server.json` 的默认主站列表。如果这个文件缺失，会退回代码内置的主站列表。

真实 socket 默认每 30 秒发一次 `0x0004` 心跳，用来维持长时间空闲连接。短脚本不用管；需要改间隔或关闭时：

```python
TdxClient(heartbeat_interval=60)
TdxClient(heartbeat_interval=None)
```

常用连接参数：

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `host` | `None` | 指定单个 7709 主站 |
| `hosts` | `None` | 指定多个 7709 主站 |
| `timeout` | `8.0` | 数字 IP 或已缓存 endpoint 的端到端请求上限，覆盖排队、连接、握手、发送、响应和一次 retry |
| `pool_size` | `1` | 连接池连接数，必须是正整数 |
| `batch_size` | `80` | `get_quote()` 自动拆批大小 |
| `probe_hosts` | `False` | 启动时是否先测速排序 |
| `heartbeat_interval` | `30.0` | 后台心跳秒数；`None` 或小于等于 0 表示关闭 |
| `max_pending_requests` | `256` | pool 中等待空闲 slot 的最大请求数；满时抛 `PoolBusyError` |
| `push_queue_size` | `1024` | 共享 push buffer 的最大帧数 |
| `push_queue_bytes` | `8 * 1024 * 1024` | 共享 push buffer 的最大 wire bytes |

自定义 hostname 的首次 DNS 解析在 Actor 外执行，标准库解析无法严格取消；它不占用 pool slot 或 TCP 连接，解析结束后会重新检查 transport epoch。数字 IP 和已缓存 endpoint 没有该例外。

`pool_size=N` 表示最多 N 个 Actor、N 个 TCP socket 和 N 个业务 wire request 同时在途。`N` 必须是正整数，非法值会直接抛出 `ValueError`，不会被自动修改。请求在全池 FIFO admission 中等待空闲 slot，由 lease 调度给首个空闲 slot，不会继续堆到慢连接后面。

多请求必须固定在同一连接时可使用 pin：

```python
with client.transport.pin() as pinned:
    first = pinned.execute(0x06B9, {"path": "zhb.zip", "offset": 0, "size": 30000})
    second = pinned.execute(0x06B9, {"path": "zhb.zip", "offset": 30000, "size": 30000})
```

pin context 独占一个 slot；context 退出或 proxy `close()` 会取消未完成 wire request 并归还 lease。它不会关闭共享 pool，也不能在 pool close/reopen 后继续使用。

## 便捷兼容方法

这一组方法是旧版常用入口的外层包装，底层仍然调用下面的分组 API。

### `get_quote(codes)`

批量查询行情快照，自动按 80 个代码拆批。底层先取 `client.quotes.get_snapshots()`，再用 `0x0547` 首次刷新补齐五档盘口。

```python
client.get_quote(["sz000001", "sh600000"])
```

### `get_quote_depth(codes)`

按代码列表直接查询五档盘口，对应 `client.quotes.get_depth()` / `0x0547` 首次刷新，单次最多 100 个代码。主站对超过 100 个代码的请求只返回前 100 条，因此接口会直接拒绝超限请求，不会返回静默截断的数据。

```python
client.get_quote_depth(["sz000001", "sh600000"])
```

### `get_legacy_quotes(codes)`

查询 `0x053e` 旧版批量行情，并自动按 80 个代码拆批。

```python
client.get_legacy_quotes(["sz000001", "sh600000"])
```

### `read_server_file(path, offset=0, size=30000)`

通过 `0x06b9` 读取一个服务器文件块。完整文件下载和 `zhb.zip` 解析见 `client.resources`。

```python
client.read_server_file("zhb.zip", offset=0, size=30000)
```

### 代码表便捷方法

```python
client.get_count("sz")
client.get_codes("sz", start=0, limit=1600)
client.get_codes_all("sz")
client.get_a_share_codes_all()
client.get_stock_codes_all()
client.get_etf_codes_all()
client.get_index_codes_all()
```

其中 A 股、股票、ETF、指数过滤使用 `0x044d` 代码表解析出的 `category` 派生字段。

### K 线便捷方法

```python
client.get_kline("day", "sz000001", count=30)
client.get_kline("sz000001", "day", count=30)
client.get_kline_all("day", "sz000001")
client.get_adjusted_kline("day", "sz000001", adjust="qfq")
client.get_adjusted_kline("week", "sz000001", adjust="hfq")
client.get_adjusted_kline("day", "sz000001", adjust="fixed_qfq", anchor_date="2024-06-03")
client.get_kline("1m", "sz000001", count=240)
```

`get_kline()` 同时支持旧版常见的 `(period, code)` 和 `(code, period)` 参数顺序。`get_adjusted_kline()` 直接使用 `0x052d` 的服务端复权参数，也可传 `anchor_date` 做定点复权；本地复权因子可用 `get_factors()` 查看。

常用周期：

| `period` | 含义 |
| --- | --- |
| `1m`, `5m`, `15m`, `30m`, `60m` | 分钟 K 线 |
| `day` | 日 K |
| `week` | 周 K |
| `month` | 月 K |
| `quarter` | 季 K |
| `year` | 年 K |
| `10m`, `2d`, `5s` | 协议层支持的自定义分钟、N 日、N 秒形式；实际覆盖以服务端返回为准 |

复权参数：

| `adjust` | 含义 |
| --- | --- |
| `None` / `none` | 不复权 |
| `qfq` / `front` | 前复权 |
| `hfq` / `back` | 后复权 |
| `fixed_qfq` / `fixed_front` | 定点前复权，需要 `anchor_date` |
| `fixed_hfq` / `fixed_back` | 定点后复权，需要 `anchor_date` |

定点复权示例：

```python
client.get_kline("day", "sz000001", adjust="fixed_qfq", anchor_date="2024-06-03")
client.get_kline("day", "sz000001", adjust="fixed_hfq", anchor_date=20240603)
```

### 分时和成交明细便捷方法

```python
client.get_minute("sz000001")
client.get_history_minute("sz000001", "2026-05-20")
client.get_trades("sz000001")
client.get_trades("sz000001", "2026-05-20")
client.get_trades_all("sz000001", "2026-05-20")
```

成交明细别名也保留：

```python
client.get_trade("sz000001")
client.get_trade_all("sz000001")
client.get_history_trade("sz000001", "2026-05-20")
client.get_history_trade_day("sz000001", "2026-05-20")
```

### 集合竞价便捷方法

```python
client.get_call_auction("sz000001")
client.get_auction_0925("sz000001", "2026-05-20")
```

`get_call_auction()` 返回 `0x056a` 当前交易日集合竞价明细。`get_auction_0925()` 从历史成交明细接口里扫描 09:25 竞价成交快照。

### 股本变迁便捷方法

```python
client.get_gbbq("sz000001")
client.get_xdxr("sz000001")
client.get_equity_changes("sz000001")
client.get_equity("sz000001", "2026-05-20")
client.get_turnover("sz000001", 123456, unit="hand")
client.get_factors("sz000001")
```

`get_gbbq()` 是旧版名称，新版底层调用 `client.corporate.capital_changes()`，对应 `0x000f`。

`get_xdxr()`、`get_equity_changes()`、`get_equity()` 是从 `0x000f` 返回的股本变迁记录里本地整理出来的。

`get_turnover()` 使用成交量和流通股本计算换手率：

```text
换手率 = 成交股数 / 流通股本 * 100
```

`unit="hand"` 表示传入成交量单位是手，`unit="share"` 表示传入成交量单位是股。

`get_factors()` 用不复权日 K 和除权除息记录计算本地复权因子。普通取复权 K 线时，仍推荐直接使用服务端复权参数：

```python
client.get_adjusted_kline("day", "sz000001", adjust="qfq")
client.bars.get("sz000001", period="day", adjust="hfq")
```

需要研究或校验本地复权时，可以用：

```python
client.get_local_adjusted_kline_all("day", "sz000001", adjust="qfq")
```

### 低频数据缓存

客户端默认缓存低频数据：代码数量、全量代码表、股本变迁和财务基础信息。实时行情、分时、成交明细、K 线每次按请求读取。

强制刷新：

```python
client.get_count("sz", refresh=True)
client.get_codes_all("sz", refresh=True)
client.get_gbbq("sz000001", refresh=True)
client.get_finance_batch(["sz000001"], refresh=True)
```

清空全部缓存：

```python
client.clear_cache()
```

### `include_raw`

部分调试场景可以传 `include_raw=True`：

```python
client.get_gbbq("sz000001", include_raw=True)
client.get_kline("day", "sz000001", include_raw=True)
client.get_history_trade("sz000001", "2026-05-20", include_raw=True)
```

大多数返回模型已经保留 `raw_payload` 或单条记录的 `record_hex`，用于抓包对照和协议解析排查。

### JSON 输出

```python
from eltdx import to_json, to_jsonable

data = to_jsonable(client.get_quote("sz000001"))
text = to_json(data, indent=2)
```

## `client.session`

### `handshake()`

连接后握手，对应 `0x000d`。

```python
client.session.handshake()
```

### `heartbeat()`

心跳保活，对应 `0x0004`。

```python
client.session.heartbeat()
```

## `client.codes`

### `count(market)`

查询某市场代码数量，对应 `0x044e`。

```python
client.codes.count("sz")
client.codes.count("sh")
client.codes.count("bj")
```

### `list(market, start=0, limit=1600)`

分页查询代码表，对应 `0x044d`。

```python
client.codes.list("sz", start=0, limit=1600)
```

### `all(market)`

拉取某市场全量代码表。

```python
client.codes.all("bj")
```

## `client.quotes`

### `get_snapshots(codes)`

按显式代码列表查询批量行情快照，对应 `0x054c`。当前实盘响应只稳定确认买一 / 卖一；需要完整五档时用 `client.get_quote()`。

```python
client.quotes.get_snapshots(["sz000001", "sh600000"])
```

别名：

```python
client.quotes.get(["sz000001", "sh600000"])
```

### `legacy(codes)`

直接调用一次 `0x053e` 旧版批量行情接口，返回 `list[LegacyQuote]`。

```python
client.quotes.legacy(["sz000001", "sh600000"])
```

### `list_by_category(category, sort_by=None, start=0, count=80, ascending=False)`

查询分类行情列表，对应 `0x054b`。

```python
client.quotes.list_by_category("沪深A股", sort_by="涨幅", count=100)
```

### `refresh(codes=None, cursors=None)`

行情增量刷新协议，对应 `0x0547`，单次最多 100 个代码。

```python
client.quotes.refresh(["sz000001"], cursors={"sz000001": 0})
```

`refresh()` 发起一次增量刷新请求。服务端主动推送帧会进入 transport 的 push queue，可用下面两个方法读取。

### `get_depth(codes)`

按代码列表直接发起一次 `0x0547` 首次刷新，返回 `QuoteRefreshPage`，适合只关心买一到买五 / 卖一到卖五的场景。单次最多 100 个代码；超过上限会抛出 `ValueError`。

```python
client.quotes.get_depth(["sz000001", "sh600000"])
```

### `poll_push(timeout=0.0, parse=False)`

读取一个未配对推送帧，默认返回原始 `ResponseFrame`。确认推送帧可直接按当前上下文解析时，可以传 `parse=True`。

```python
frame = client.quotes.poll_push(timeout=0.5)
event = client.quotes.poll_push(timeout=0.5, parse=True)
```

push queue 同时受帧数和字节数限制。满时会丢弃最旧帧以保留最新行情，并在下一次 `poll_push()` 或 `drain_pushes()` 抛出一次 `PushOverflowError`；捕获后继续读取即可，异常消息包含累计丢弃数。

### `drain_pushes(parse=False)`

取出当前队列里已经收到的全部推送帧。

```python
frames = client.quotes.drain_pushes()
```

`close()` 成功返回时旧 Actor、TCP socket、selector、wakeup、ticket、waiter 和 lease 都已结束。若 1 秒内无法证明完成，会抛 `TransportCloseTimeoutError` 并进入 failed-closed 状态；请创建新的 `TdxClient`，不要复用该实例。

## `client.resources`

### `read(path, offset=0, size=30000)`

通过 `0x06b9` 读取一个服务器文件块，返回 `FileContentChunk`。这个入口不循环下载整文件。

```python
chunk = client.resources.read("zhb.zip", offset=0, size=30000)
```

### `download_file(path, chunk_size=30000, max_bytes=None)`

循环调用 `0x06b9` 并按实际返回长度拼接完整文件，返回 `bytes`。`chunk_size` 范围为 `1..60000`；`max_bytes` 可限制最多下载的字节数。

```python
payload = client.resources.download_file("zhb.zip")
```

### `read_stats(path="zhb.zip", chunk_size=30000)`

下载并解析 `zhb.zip` 中的 `tdxstat.cfg` 和 `tdxstat2.cfg`，返回 `TdxStatsResource`。两个 CFG 使用 GBK 解码，结构化字段可通过 `stats.row(market_id, code)` 查询。

```python
stats = client.resources.read_stats()
stat, stat2 = stats.row(0, "000001")
```

该解析只针对已知的 `zhb.zip` 格式；其他服务器文件由 `download_file()` 返回原始 bytes。

## `client.bars`

### `get(code, period="day", start=0, count=800, adjust=None, anchor_date=None, include_raw=False)`

查询 K 线 / 周期线，对应 `0x052d`。

```python
client.bars.get("sz000001", period="day", count=800)
client.bars.get("sz000001", period="week", count=200)
client.bars.get("sz000001", period="month", count=120)
client.bars.get("sz000001", period="quarter", count=80)
client.bars.get("sz000001", period="year", count=30)
client.bars.get("sz000001", period="1m", count=800)
client.bars.get("sz000001", period="day", adjust="qfq", count=800)
```

返回 `KlineSeries`，主要字段：

| 字段 | 含义 |
| --- | --- |
| `period_name` | 服务端返回周期名 |
| `adjust_mode` | 复权模式，`none`、`qfq`、`hfq`、`fixed_qfq`、`fixed_hfq` |
| `bars` | K 线记录列表 |
| `bars[].time` | K 线时间 |
| `bars[].open/high/low/close` | 开高低收 |
| `bars[].volume_lots` | 成交量，单位手 |
| `bars[].amount` | 成交额 |

周期和复权参数同上。定点复权可同时传 `anchor_date`。

### `all(code, period="day", adjust=None, page_size=800, max_pages=200, include_raw=False)`

分页拉取 K 线，直到服务端返回短页。`max_pages` 用来避免异常情况下无限循环。

```python
client.bars.all("sz000001", period="day")
```

## `client.minutes`

### `today(code, include_raw=False)`

查询当日分时，对应 `0x0537`。

```python
client.minutes.today("sz000001")
```

### `history(code, trading_date, include_raw=False)`

查询指定日期历史分时，对应 `0x0fb4`。

```python
client.minutes.history("sz000001", "2026-05-20")
```

### `recent(code, trading_date, include_raw=False)`

查询近期历史分时，对应 `0x0feb`。

```python
client.minutes.recent("sz000001", "2026-05-20")
```

### `aux(code, kind, include_raw=False)`

查询分时副图数据，对应 `0x051b`。

```python
client.minutes.aux("sz000001", kind="buy_sell_strength")
client.minutes.aux("sz000001", kind="volume_comparison")
```

### `sparkline(code, selector=1, window=20, include_raw=False)`

查询单标的小走势图，对应 `0x0fd1`。

```python
client.minutes.sparkline("sz000001", selector=1)
```

## `client.trades`

### `today(code, start=0, count=1800, include_raw=False)`

查询当日成交明细，对应 `0x0fc5`。

```python
client.trades.today("sz000001", start=0, count=1800)
```

### `history(code, trading_date, start=0, count=2000, include_raw=False)`

查询历史成交明细增强接口，对应 `0x0fc6`。

```python
client.trades.history("sz000001", "2026-05-20")
```

## `client.auctions`

### `series(code, include_raw=False)`

查询当前交易日集合竞价明细，对应 `0x056a`。

```python
client.auctions.series("sz000001")
```

## `client.corporate`

### `capital_changes(code, include_raw=False)`

查询股本变迁 / 除权相关数据，对应 `0x000f`。

```python
client.corporate.capital_changes("sz000001")
```

### `finance_batch(codes, fields=None, include_raw=False)`

批量查询财务字段，对应 `0x0010`。`fields` 只过滤本地返回字段，底层仍按 `0x0010` 请求完整记录。

```python
client.corporate.finance_batch(["sz000001", "sh600000"])
client.corporate.finance_batch(["sz000001"], fields=["流通股本", "total_shares"])
```

## `client.limits`

### `special(start_index=0)`

查询特殊品种涨跌停限制表，对应 `0x0452`。

```python
client.limits.special(start_index=0)
```

`0x0452` 按表内行号分页取记录。需要查询某个代码时，先扫描建本地索引：

```python
records = client.limits.scan_special()
```

## `client.f10`

`client.f10` 走 `7615/TQLEX` HTTP 网关，独立于 `7709` socket 握手。它适合查询 F10 资料、题材、公告、财务报表和估值数据。

```python
client.f10.company_profile("000034")
client.f10.hot_topics("000034")
client.f10.announcements("000034")
client.f10.finance_report("000034")
client.f10.valuation("000034")
```

所有方法返回 `F10Response`，常用数据在第一张表的 `rows`：

```python
response = client.f10.hot_topics("000034")
print(response.entry)
print(response.rows[:3])
```

需要直接调用 Entry 时，可以使用通用 TQLEX 调用：

```python
client.f10.call("CWServ.tdxf10_gg_gsgk", params=["8", "000034", ""])
```

完整 F10 方法表见 [F10_7615.md](F10_7615.md)。

## `client.helpers`

`client.helpers` 提供常用问题的组合调用。

```python
with TdxClient(timeout=3) as client:
    profiles = client.helpers.stock_profile_table(["sz000001", "sh600000"])
    shortline = client.helpers.shortline_indicators(["sz000001", "sh600000"])
    topics = client.helpers.stock_topics("000034")
    stocks = client.helpers.topic_stocks("000034", topic_name="存储芯片")
    auction = client.helpers.auction_data("sz000001", "2026-05-20")
```

- [想拿某个或某些股票的表头信息怎么办？](helpers/股票信息汇总.md)
- [想查询某个股票都有哪些概念板块怎么办？](helpers/个股概念板块.md)
- [想查询某个概念板块都有哪些股票怎么办？](helpers/概念板块成分股.md)
- [想拿集合竞价数据怎么办？](helpers/竞价数据.md)
- [想给一批股票整理行情表怎么办？](helpers/批量行情表.md)
- [想拿流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比和几天几板怎么办？](helpers/短线指标.md)
- [想拿复权或不复权 K 线怎么办？](helpers/复权K线.md)
