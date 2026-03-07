# Debug Guide

这份文档用于说明 `eltdx` 的调试方法、raw 数据查看方式、smoke 脚本用法，以及一些最常见的问题排查思路。

配套阅读：

- API 用法：[`API_REFERENCE.md`](./API_REFERENCE.md)
- 可运行示例：[`EXAMPLES.md`](./EXAMPLES.md)
- 字段说明：[`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
- 维护者资料：[`maintainers/README.md`](./maintainers/README.md)

## 1. 最常用的调试入口

### 1.1 先跑 smoke 脚本

常用命令：

```bash
python scripts/smoke_codes.py --exchange sh --limit 10
python scripts/smoke_minute.py --code sz000001 --date 2026-03-06
python scripts/smoke_trade.py --code sz000001 --date 2026-03-06 --count 50
python scripts/smoke_kline.py --code sz000001 --period day --count 10
python scripts/smoke_call_auction.py --code sz000001
```

如果你想一次跑完整条在线链路：

```bash
python scripts/smoke_live_all.py
```

如果你想验证发布后的 wheel 能否在隔离环境正常导入：

```bash
python scripts/smoke_isolated_install.py
```

### 1.2 用 `include_raw=True` 看原始数据

下面这些方法支持查看原始十六进制：

| 方法 | 可查看字段 |
| --- | --- |
| `get_minute()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_history_minute()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_trades()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_history_trade()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_kline()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_adjusted_kline()` | `raw_frame_hex`, `raw_payload_hex` |
| `get_call_auction()` | `raw_frame_hex`, `raw_payload_hex`, `items[].raw_hex` |
| `get_gbbq()` | `raw_frame_hex`, `raw_payload_hex` |

## 2. raw 调试示例

### 2.1 分时

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_minute("sz000001", include_raw=True)
    print(minute.raw_frame_hex)
    print(minute.raw_payload_hex)
    print(minute.items[0].time, minute.items[0].price_milli, minute.items[0].price)
```

适合场景：

- 核对解析后的分钟价格是否正确
- 比较原始 hex 和 dataclass 字段
- 排查某只证券的分时解析异常

### 2.2 K 线

```python
from eltdx import TdxClient

with TdxClient() as client:
    response = client.get_kline("sz000001", "day", count=10, include_raw=True)
    print(response.raw_frame_hex)
    print(response.raw_payload_hex)
    print(response.items[0].open_price_milli, response.items[0].open_price)
```

### 2.3 集合竞价

```python
from eltdx import TdxClient

with TdxClient() as client:
    response = client.get_call_auction("sz000001", include_raw=True)
    first = response.items[0]
    print(first.raw_hex)
    print(first.price_milli, first.price)
    print(first.match, first.unmatched, first.flag)
```

## 3. 服务器地址调试

### 3.1 指定单个地址

```python
from eltdx import TdxClient

with TdxClient(host="124.71.187.122:7709") as client:
    print(client.get_quote("sz000001")[0].last_price)
```

### 3.2 指定多个地址

```python
from eltdx import TdxClient

hosts = [
    "124.71.187.122:7709",
    "122.51.120.217:7709",
    "111.229.247.189:7709",
]

with TdxClient(hosts=hosts, pool_size=2) as client:
    print(client.get_quote("sz000001")[0].last_price)
```

说明：

- `host=` 适合手动测试某个固定地址
- `hosts=` 适合交给客户端自动挑选和分发请求
- 不传时会回退到库内默认地址列表

## 4. 常见问题

### 4.1 为什么 `get_count("sh")` 看起来特别大？

因为它表示的是底层代码表条目数，不是股票总数。

如果你需要更接近股票口径的数量，优先使用：

- `get_stock_count("sh")`
- `get_a_share_count("sh")`

### 4.2 `get_codes()` / `get_codes_all()` 为什么看起来不全是股票？

因为底层代码表会混有股票、指数、板块分类项、ETF、基金、债券回购等条目。

如果你想直接拿常用代码集合去拉行情，优先使用：

- `get_stock_codes_all()`
- `get_a_share_codes_all()`
- `get_etf_codes_all()`
- `get_index_codes_all()`

### 4.3 `sh900xxx` 和 `sz200xxx` 是什么？

它们通常对应 B 股代码，不是 A 股代码。

如果你只想拿 A 股，优先使用：

- `get_a_share_codes_all()`
- `get_a_share_count()`

### 4.4 为什么 `Quote.server_time` 有时是 `None`？

因为 `server_time` 是尽力恢复的时间字段：

- 底层原始值在 `server_time_raw`
- 如果原始值不适合转换成合法时间，`server_time` 允许为 `None`
- 排查时优先同时查看 `server_time_raw` 和 `server_time`

### 4.5 `with TdxClient()` 是不是强制场景？

不是。

- 一次性任务：推荐 `with`
- 长连接实时场景：也可以手动 `connect()` / `close()`

## 5. 脚本说明

| 脚本 | 作用 |
| --- | --- |
| `scripts/smoke_codes.py` | 验证代码表和市场计数相关接口 |
| `scripts/smoke_minute.py` | 验证实时 / 历史分时 |
| `scripts/smoke_trade.py` | 验证实时 / 历史逐笔 |
| `scripts/smoke_kline.py` | 验证 K 线与指数 K 线 |
| `scripts/smoke_call_auction.py` | 验证集合竞价 |
| `scripts/smoke_gbbq.py` | 验证公司行为数据 |
| `scripts/smoke_equity.py` | 验证股本变化与股本记录 |
| `scripts/smoke_adjustment.py` | 验证复权因子与复权 K 线 |
| `scripts/smoke_trade_kline.py` | 验证逐笔聚合分钟 K 线 |
| `scripts/smoke_live_all.py` | 一次串联跑主要在线能力 |
| `scripts/smoke_isolated_install.py` | 验证构建后的 wheel 在隔离环境可导入 |

## 6. 如果还想看更深入的资料

如果你想继续看：

- 最近一次联网验证结果
- 架构说明
- 发布流程
- 字段解释的实现依据

请继续看：[`maintainers/README.md`](./maintainers/README.md)
