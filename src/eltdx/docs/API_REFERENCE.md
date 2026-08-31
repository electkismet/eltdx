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
    quotes = client.helpers.full_quotes(["sz000001", "sh600000"])
```

也可以使用连接池和主站测速：

```python
with TdxClient.from_hosts(
    server_count=2,
    connections_per_server=4,
    probe_hosts=True,
    timeout=3,
) as client:
    quotes = client.helpers.full_quotes(["sz000001", "sh600000"])
```

`probe_hosts=True` 会在第一次真正建立 Native Engine 前，用 TCP connect 测一遍全部候选主站，把连得上的、延迟低的排在前面。默认开启测速；只构造客户端但不连接时不会触发网络操作。

不传 `host` / `hosts` 时，客户端会读取包内 `tdx_server.json` 的43台默认主站。如果这个文件缺失，会退回代码内置列表。测速结果会原子写入当前用户数据目录的 `tdx_server_ranking.json`，下次启动先复用已保存的排名再刷新；软件升级不会覆盖这张本地排名表。可调用 `eltdx.hosts.refresh_server_ranking()` 手动重新测速并保存。

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
| `server_count` | `2` | 从持久测速排名中使用的服务器数量 |
| `connections_per_server` | `4` | 每台选中服务器的 TCP 连接数；未显式设置 `pool_size` 时据此计算总数 |
| `pool_size` | `None`，自动为 `8` | 兼容参数，显式指定 TCP/Slot 总数时在选中服务器之间尽量平均分配 |
| `runtime_workers` | `None` | 自动取 `min(pool_size, 系统允许的逻辑处理器数)`；可手动指定1到 `pool_size` |
| `max_connections_per_host` | `None` | 自动按分布计算每台服务器的活动连接硬上限 |
| `connect_concurrency` | `None` | 自动计算全局同时建连和握手数量，最大默认32 |
| `connect_concurrency_per_host` | `None` | 每台服务器同时建连和握手数量，默认最多2 |
| `probe_hosts` | `True` | 第一次建 Engine 前是否测速、持久化并排序候选主站 |
| `heartbeat_interval` | `30.0` | 后台心跳秒数；`None` 表示关闭，非 `None` 时必须大于 0 |
| `max_pending_requests` | `256` | pool 中等待空闲 slot 的最大请求数；满时抛 `PoolBusyError` |
| `push_queue_size` | `1024` | 共享 push buffer 的最大帧数 |
| `push_queue_bytes` | `64 * 1024 * 1024` | 共享 push buffer 的最大 wire bytes |
| `global_raw_bytes` | `None` | 自动按 Slot 数增长、最高256 MiB的 Engine raw 预算 |
| `global_decoded_bytes` | `None` | 自动按 Slot 数增长、最高2 GiB的 Engine decoded 预算 |

自定义 hostname 的首次 DNS 解析在 native Engine request deadline 外执行，标准库解析无法严格取消；它不占用 pool Slot 或 TCP 连接，解析结束后会重新检查 transport epoch 和 close 状态。数字 IP 和已缓存 endpoint 没有该例外。

默认不传 `pool_size`，由最快2台服务器乘以每台4个 Slot 得到8。`pool_size=N` 继续表示 native Engine 最多拥有 N 个 Rust Slot、N 个 TCP socket 和 N 个业务 wire request 同时在途；它不是 worker 数。显式同时传 `pool_size`、`server_count` 和 `connections_per_server` 时，三者乘积必须一致。请求在 Supervisor 的全池 FIFO admission 中等待空闲 Slot；等待 permit 和 active lease 分开计数。

多请求必须固定在同一连接时可使用 pin：

```python
with client.transport.pin() as pinned:
    first = pinned.execute(0x06B9, {"path": "zhb.zip", "offset": 0, "size": 30000})
    second = pinned.execute(0x06B9, {"path": "zhb.zip", "offset": 30000, "size": 30000})
```

pin context 独占一个 slot；context 退出或 proxy `close()` 会取消未完成 wire request 并归还 lease。它不会关闭共享 pool，也不能在 pool close/reopen 后继续使用。

## 组合与便捷方法

这一组方法组合底层分组 API，提供分页、五档补齐、解析和本地计算等常用能力。

### `client.helpers.full_quotes(codes)`

批量查询完整五档行情，自动按 80 个代码拆批，底层组合 `0x054c` 基础快照和 `0x0547` 首次刷新。

```python
client.helpers.full_quotes(["sz000001", "sh600000"])
```

### `client.quotes.get_depth(codes)`

按代码列表直接发起一次 `0x0547` 刷新，首次刷新用于建立实时五档，后续可通过推送增量更新。

```python
client.quotes.get_depth(["sz000001", "sh600000"])
```

### 代码表便捷方法

```python
client.codes.count("sz")
client.codes.list("sz", start=0, limit=1600)
client.codes.all("sz")
client.codes.all_a_shares()
client.codes.all_stocks()
client.codes.all_etfs()
client.codes.all_indices()
```

其中 A 股、股票、ETF、指数过滤使用 `0x044d` 代码表解析出的 `category` 派生字段。

### K 线便捷方法

```python
client.bars.get("sz000001", period="day", count=30)
client.bars.get("sz000001", period="day", all_pages=True, page_size=800)
client.bars.get("sz000001", period="day", adjust="qfq")
client.bars.get("sz000001", period="week", adjust="hfq")
client.bars.get("sz000001", period="day", adjust="fixed_qfq", anchor_date="2024-06-03")
```

`all_pages=False` 取一页；`all_pages=True` 自动拉到空页并合并。复权参数直接交给 `0x052d` 主站计算。本地审计使用 `client.corporate.adjustment_factors()`，它返回完整的 `scale + offset` 仿射系数。

常用周期为 `1m/5m/15m/30m/60m`、`day/week/month/quarter/year`。复权模式为 `none`、`qfq`、`hfq`、`fixed_qfq`、`fixed_hfq`；定点模式需要 `anchor_date`。

### 分时和成交明细便捷方法

```python
client.minutes.today("sz000001")
client.minutes.history("sz000001", "2026-05-20")
client.trades.today("sz000001")
client.trades.history("sz000001", "2026-05-20")
client.trades.all_history("sz000001", "2026-05-20")
client.trades.all_history(["sz000001", "sh600000"], "2026-05-20")
```

成交明细提供单页和完整分页两组入口：

```python
client.trades.today("sz000001")
client.trades.all_today("sz000001")
client.trades.history("sz000001", "2026-05-20")
client.trades.all_history("sz000001", "2026-05-20")
```

### 集合竞价便捷方法

```python
client.auctions.series("sz000001")
client.auctions.series("sz000001", "2026-05-20")
client.trades.opening_match_history("sz000001", "2026-05-20")
client.trades.opening_match_today(["sz000001", "sh600000"])
```

`client.auctions.series()` 返回 `0x056a` 主站保存的当日或历史集合竞价过程快照；不传日期查询当日，传入日期查询历史。`client.trades.opening_match_today()` 和 `opening_match_history()` 分别从 `0x0fc5`、`0x0fc6` 只取 09:25 正式开盘撮合。

成交入口传入代码列表时返回以规范化完整代码为键的结果字典；底层仍逐只请求，`batch_size` 控制同时查询的股票数，默认跟随连接池大小。

### 股本变迁和本地复权系数

```python
changes = client.corporate.capital_changes("sz000001")
factors = client.corporate.adjustment_factors("sz000001")
anchored = client.corporate.adjustment_factors("sz000001", anchor_date="2024-05-31")
```

`capital_changes()` 返回标签 `1..15` 的广义权息/股本变迁原始业务记录。`adjustment_factors()` 根据其中的标签 `1` 事件，计算每个除权事件日期的前、后复权仿射系数：

```text
adjusted = round(raw * scale + offset, 2)
```

普通复权 K 线直接使用 `client.bars.get(..., adjust="qfq" / "hfq")`。

### 低频数据缓存

Helpers 只缓存组合查询内部使用的财务批次、证券表和已验证的短线统计资源。股本变迁、代码数量、代码表、直接财务查询、实时行情、分时、成交明细和 K 线每次按请求读取。

```python
client.helpers.shortline_indicators("sz000001", refresh_stats=True)
client.clear_cache()
```

### `include_raw`

部分调试场景可以传 `include_raw=True`：

```python
client.corporate.capital_changes("sz000001", include_raw=True)
client.bars.get("sz000001", period="day", include_raw=True)
client.trades.history("sz000001", "2026-05-20", include_raw=True)
```

大多数返回模型已经保留 `raw_payload` 或单条记录的 `record_hex`，用于抓包对照和协议解析排查。

### JSON 输出

```python
from eltdx import to_json, to_jsonable

data = to_jsonable(client.helpers.full_quotes("sz000001"))
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

查询某市场完整代码表条数，对应 `0x044e`。结果不限于 A 股；仅统计 A 股时使用 `a_share_count(market)`。

```python
client.codes.count("sz")
client.codes.count("sh")
client.codes.count("bj")
client.codes.a_share_count("sh")
```

### `list(market, start=0, limit=1600)`

分页查询代码表，对应 `0x044d`。

```python
client.codes.list("sz", start=0, limit=1600)
```

### `all(market)`

自动分页拉取某市场全量代码表，不需要先调用 `count()`。

```python
client.codes.all("bj")
client.codes.a_shares("bj")
client.codes.all_a_shares()
```

## `client.quotes`

### `client.quotes.get_snapshots(codes)`

按显式代码列表查询一次 `0x054c` 基础快照。当前实盘响应只稳定确认买一 / 卖一；普通业务需要完整行情时使用 `client.helpers.full_quotes()`，直接操作原生五档刷新时才使用 `client.quotes.get_depth()`。

```python
client.quotes.get_snapshots(["sz000001", "sh600000"])
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

### `client.quotes.get_depth(codes)`

按代码列表直接发起一次 `0x0547` 刷新，等价于 `refresh(codes, cursors={})`，返回 `QuoteRefreshPage`。首次刷新用于建立实时五档，后续可由推送队列增量更新；单次最多 100 个代码。

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

`close()` 成功返回时旧 Rust Slot task、TCP socket、request owner、waiter、pin、push buffer 和 runtime thread 都已结束。若 1 秒内无法证明完成，会抛 `TransportCloseTimeoutError` 并进入 `FAILED_CLOSING`；后续可以再次 `close()` 完成清理，但该实例不能 reopen，请创建新的 `TdxClient`。

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

### `get(code, period="day", start=0, count=800, adjust=None, anchor_date=None, kind="stock", include_raw=False, all_pages=False, page_size=800, max_pages=200)`

查询 K 线 / 周期线，对应 `0x052d`。

```python
client.bars.get("sz000001", period="day", count=800)
client.bars.get("sz000001", period="day", adjust="qfq")
client.bars.get("sz000001", period="day", all_pages=True, page_size=800)
```

`all_pages=False` 时校验并使用 `count`，只请求一页。`all_pages=True` 时使用 `page_size` 自动请求到空页，`max_pages` 防止异常情况下无限循环，返回合并后的 `KlineSeries`。短页不会提前停止。

返回字段包括 `period_name`、`adjust_mode`、`bars`；每根 K 线提供 `time`、`open/high/low/close`、`volume_lots` 和 `amount`。

## `client.minutes`

### `today(code, include_raw=False)`

查询主站当前保存的分时，对应 `0x0537`。凌晨、周末或节假日可能返回最近交易日数据。

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

### `today(code, start=0, count=1800, include_raw=False, batch_size=None)`

查询主站当前保存的混合明细，对应 `0x0fc5`。凌晨、周末或节假日可能返回最近交易日数据。`ticks` 原样保留 `status=8` 竞价快照；`actual_trades` 排除这些非成交快照，并保留 09:25、15:00 和 `status=5` 盘后固定价格真实成交。

```python
client.trades.today("sz000001", start=0, count=1800)
client.trades.today(["sz000001", "sh600000"])
```

`after_hours_trades` 可单独取得 15:05-15:30、`status=5` 的盘后固定价格成交。完整秒级竞价过程和竞价数量使用 `client.auctions.series()`。

### `history(code, trading_date, start=0, count=1800, include_raw=False, batch_size=None)`

查询历史混合明细增强接口，对应 `0x0fc6`。原始 `ticks` 与真实成交视图的分类规则和当日接口一致。

```python
client.trades.history("sz000001", "2026-05-20")
client.trades.history(["sz000001", "sh600000"], "2026-05-20")
```

## `client.auctions`

### `series(code, date=None, include_raw=False)`

查询主站保存的当日或历史集合竞价过程快照，对应 `0x056a`；不传日期查询当日，传入日期查询历史。它不是逐笔成交接口，即使返回 `09:25:00` 也仍按快照解释。

```python
client.auctions.series("sz000001")
client.auctions.series("sz000001", "2026-05-20")
```

## `client.money_flow`

### `daily(code, include_raw=False)`

读取单只证券最近的日资金流向分档数据，对应 `0x0ffc`，返回 `MoneyFlowBlock`。内部路由字段由协议层自动处理，用户不需要传 `route` 或 `channel`。

```python
flow = client.money_flow.daily("sz000063")
print(flow.records[0].date, flow.records[0].main_net)
```

完整参数和返回字段见 [资金流向日数据](methods/0x0ffc-资金流向日数据接口.md)。

## `client.corporate`

### `capital_changes(code_or_codes, include_raw=False, batch_size=75)`

查询标签 `1..15` 的广义权息和股本变迁资料，对应 `0x000f`。传单个代码返回 `CapitalChangeBlock`；传代码列表默认按 75 只拆批，`batch_size` 可设为 `1..200`。超量批次按连接池 Slot 数并发请求；主站按响应大小截断时会自动补拉未返回的代码。

```python
client.corporate.capital_changes("sz000001")
client.corporate.capital_changes(["sz000001", "sh600000", "bj920000"])
```

### `adjustment_factors(code_or_codes, anchor_date=None, *, start_date=None, batch_size=75)`

传单个代码返回 `AdjustmentFactorResponse`；传代码列表返回 `AdjustmentFactorBatch`。批量调用复用批量 `0x000f` 返回的股票块，在本地逐只计算，不会为每只股票单独请求。每个除权事件日期一条 `AdjustmentFactor`，包含 `qfq_scale/qfq_offset` 与 `hfq_scale/hfq_offset`，用于应用到本地不复权 K 线。

```python
client.corporate.adjustment_factors(
    "sz000858",
    anchor_date="2024-06-03",
    start_date="1998-04-27",
)
```

应用到本地不复权 K 线时，前复权选择第一条满足 `bar_date < factor.date` 的系数，后复权选择最后一条满足 `factor.date <= bar_date` 的系数，再计算 `round(raw * scale + offset, 2)`。直接获取服务端复权 K 线时，使用 `client.bars.get(..., adjust="none" / "qfq" / "hfq")`。

### `finance_batch(codes, fields=None, include_raw=False)`

批量查询财务字段，对应 `0x0010`。`fields` 只过滤本地返回字段，底层仍请求完整记录。

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
- [想拿流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比和几天几板怎么办？](helpers/短线指标.md)
- [K 线、自动分页和服务端复权](methods/7709-K线周期线.md)
- [资金流向日数据](methods/0x0ffc-资金流向日数据接口.md)
