<div align="center">
  <h1>eltdx</h1>
  <p><strong>通达信行情协议的 Python 客户端库</strong></p>
  <p>面向 PyPI 发布与实际行情接入场景，提供统一入口、明确方法名、稳定 dataclass 返回模型，以及可选原始十六进制调试能力。</p>
  <p>
    <a href="./docs/README.md"><img alt="Docs" src="https://img.shields.io/badge/Docs-完整文档-2ea44f"></a>
    <a href="./docs/API_REFERENCE.md"><img alt="API Reference" src="https://img.shields.io/badge/API-Reference-0A66C2"></a>
    <a href="./docs/FIELD_REFERENCE.md"><img alt="Field Reference" src="https://img.shields.io/badge/Fields-Reference-8A2BE2"></a>
    <a href="./docs/EXAMPLES.md"><img alt="Examples" src="https://img.shields.io/badge/Examples-Runnable-orange"></a>
  </p>
  <p>
    <img alt="Python 3.10+" src="https://img.shields.io/badge/Python-3.10%2B-3776AB">
    <img alt="MIT" src="https://img.shields.io/badge/License-MIT-blue.svg">
    <img alt="Status Alpha" src="https://img.shields.io/badge/Status-Alpha-orange">
    <img alt="Build Hatchling" src="https://img.shields.io/badge/Build-Hatchling-FF69B4">
  </p>
</div>

---

## 简介

`eltdx` 是一个面向 Python 的通达信行情客户端库，目标是让外部调用尽量简单：

- 一个主入口：`TdxClient`
- 明确的方法名：`get_quote()`、`get_minute()`、`get_trades()`、`get_kline()` 等
- 统一返回 dataclass：不直接向外暴露裸 `dict`
- 所有时间字段统一转换为 Python 原生 `date` / `datetime`
- 价格统一保留双形态：浮点值 + `*_milli` 整数值
- 支持 `include_raw=True` 输出原始 `frame/payload` 十六进制，便于调试与比对

当前版本重点覆盖：

- 行情快照
- 分时
- 逐笔
- K 线
- 集合竞价
- 公司行为 / 股本变化 / 复权因子
- 常用 helper 与服务层封装

## 特性

### 对调用者友好

- `with TdxClient() as client:` 支持上下文管理器，适合一次性抓取任务
- 也支持手动 `connect()` / `close()`，适合长连接实时场景
- 默认两条长连接，适合较高频或批量查询

### 对批量请求友好

- `get_quote()` 内置自动分批，默认每批 `80` 个代码
- 默认会把批次分发到两条连接上执行，避免一次传入几千个代码时直接崩掉
- 可通过 `batch_size` 与 `pool_size` 调整行为

### 对调试友好

- 关键协议接口支持 `include_raw=True`
- 支持查看 `raw_frame_hex`、`raw_payload_hex`
- 适合人工核对、原始报文比对、问题定位

### 对数据处理友好

- 返回统一 dataclass 模型
- 时间字段已转换为 Python 原生类型
- 价格字段保留 `price` 与 `price_milli` 双形态
- 文档中提供字段中文解释、真实样例与进阶说明入口

## 安装

### 方式 1：本地开发安装

```bash
pip install -e .
```

### 方式 2：构建 wheel 后安装

```bash
python -m build
pip install dist/eltdx-0.1.0-py3-none-any.whl
```

### 方式 3：发布到 PyPI 后安装

```bash
pip install eltdx
```

当前环境要求：

- Python `>=3.10`

仓库维护与发布流程见：[`docs/maintainers/PUBLISHING.md`](./docs/maintainers/PUBLISHING.md)

## 30 秒上手

### 1）最简单的用法：`with`

```python
from eltdx import TdxClient

with TdxClient() as client:
    quotes = client.get_quote(["sz000001", "sh600000"])
    for quote in quotes:
        print(quote.exchange, quote.code, quote.last_price, quote.server_time)
```

### 2）自定义服务器地址

```python
from eltdx import TdxClient

with TdxClient(host="124.71.187.122:7709") as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.last_price)
```

### 3）传多个地址，交给客户端自动处理

```python
from eltdx import TdxClient

client = TdxClient(
    hosts=[
        "124.71.187.122:7709",
        "122.51.120.217:7709",
    ],
    pool_size=2,
    batch_size=80,
)

with client:
    quotes = client.get_quote(["sz000001", "sh600000", "sh601398"])
    print(len(quotes))
```

### 4）查看原始协议 hex

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_minute("sz000001", include_raw=True)
    print(minute.raw_frame_hex)
    print(minute.raw_payload_hex)
```

## 常见调用场景

### 获取行情快照

```python
from eltdx import TdxClient

with TdxClient() as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.last_price)
    print(quote.buy_levels[0].price, quote.buy_levels[0].number)
```

### 获取实时分时 / 历史分时

```python
from eltdx import TdxClient

with TdxClient() as client:
    live_minute = client.get_minute("sz000001")
    history_minute = client.get_minute("sz000001", "2026-03-06")

    print(live_minute.trading_date, live_minute.items[0].time, live_minute.items[0].price)
    print(history_minute.trading_date, history_minute.items[0].time, history_minute.items[0].price)
```

### 获取实时逐笔 / 历史逐笔

```python
from eltdx import TdxClient

with TdxClient() as client:
    live_page = client.get_trades("sz000001", start=0, count=100)
    history_page = client.get_trades("sz000001", "2026-03-06", start=0, count=200)

    print(live_page.items[0].time, live_page.items[0].price, live_page.items[0].side)
    print(history_page.items[0].time, history_page.items[0].price, history_page.items[0].side)
```

### 获取 K 线 / 指数 K 线

```python
from eltdx import TdxClient

with TdxClient() as client:
    stock_kline = client.get_kline("sz000001", "day", count=10)
    index_kline = client.get_kline("sh000001", "day", kind="index", count=10)

    print(stock_kline.items[0].close_price)
    print(index_kline.items[0].up_count, index_kline.items[0].down_count)
```

### 获取集合竞价

```python
from eltdx import TdxClient

with TdxClient() as client:
    auction = client.get_call_auction("sz000001", include_raw=True)
    item = auction.items[0]
    print(item.time, item.price, item.match, item.unmatched, item.flag)
```

## API 总览

### 生命周期

- `connect()`：建立连接池
- `close()`：关闭连接池
- `with TdxClient() as client:`：自动连接与关闭

### 市场 / 代码表

- `get_count(exchange)`：返回单市场代码表条目数
- `get_codes(exchange, start=0, limit=1000)`：分页读取代码表
- `get_codes_all(exchange)`：读取单市场完整代码表
- `get_stock_count(exchange)`：股票类代码数量 helper
- `get_a_share_count(exchange)`：A 股数量 helper
- `get_stock_codes_all()`：股票代码清单 helper
- `get_a_share_codes_all()`：A 股代码清单 helper
- `get_etf_codes_all()`：ETF 代码清单 helper
- `get_index_codes_all()`：指数代码清单 helper

### 行情

- `get_quote(codes)`：实时快照
- `get_minute(code, date=None)`：实时 / 历史分时统一入口
- `get_history_minute(code, date)`：历史分时兼容别名
- `get_trades(code, date=None, start=0, count=1800)`：实时 / 历史逐笔分页
- `get_trades_all(code, date=None)`：实时 / 历史逐笔全量
- `get_trade()` / `get_history_trade()` / `get_trade_all()` / `get_history_trade_day()`：逐笔兼容别名
- `get_kline(code, freq, start=0, count=800, kind="stock")`：K 线分页
- `get_kline_all(code, freq, kind="stock")`：K 线全量
- `get_adjusted_kline()` / `get_adjusted_kline_all()`：复权 K 线
- `get_call_auction(code)`：集合竞价

### 公司行为 / 股本 / 复权

- `get_gbbq(code)`：原始公司行为记录
- `get_xdxr(code)`：除权除息记录
- `get_equity_changes(code)`：股本变化记录
- `get_equity(code, on=None)`：指定日期股本记录
- `get_turnover(code, volume, on=None, unit="hand")`：换手率计算 helper
- `get_factors(code)`：复权因子序列

### 派生 helper

- `get_trade_minute_kline(code)`：逐笔聚合成分钟 K 线
- `get_history_trade_minute_kline(code, date)`：历史逐笔聚合分钟 K 线

### 服务层对象

- `CodesService`
- `WorkdayService`
- `GbbqService`

## 字段说明怎么找

如果你最关心的是“这个字段到底是什么意思”，建议按下面顺序看：

1. [`docs/API_REFERENCE.md`](./docs/API_REFERENCE.md)
   - 看每个函数怎么调用
   - 看参数和返回模型
2. [`docs/FIELD_REFERENCE.md`](./docs/FIELD_REFERENCE.md)
   - 看字段中文含义
   - 看真实样例
   - 看原始值 -> 解析值对照
3. [`docs/DEBUG_GUIDE.md`](./docs/DEBUG_GUIDE.md)
   - 看 `include_raw=True` 怎么调试
   - 看 raw hex 与解析结果怎么比对

如果你还想看实现依据、联网验证记录和发布资料，再看：[`docs/maintainers/README.md`](./docs/maintainers/README.md)

## 重要语义边界

### 1）`get_count(exchange)` 不是股票总数

它返回的是**底层代码表条目数**，不是“这个市场一共有多少只股票”。

### 2）`get_codes*()` 不是官方证券分类总表

代码表里会混有：

- 股票
- 指数
- 板块分类项
- ETF
- 基金
- 债券回购
- 其他条目

因此：

- `get_codes*()` 适合拿来做底层抓取
- `get_stock_codes_all()` / `get_a_share_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()` 更适合直接喂给行情接口

### 3）指数 K 线建议显式传 `kind="index"`

```python
with TdxClient() as client:
    index_kline = client.get_kline("sh000001", "day", kind="index")
```

这样 `up_count` / `down_count` 等字段的含义更清楚。

### 4）`with` 关闭是可选的，不是强制的

- 一次性拉数据：推荐 `with`
- 长连接实时场景：可以手动持有 `client`，最后再 `close()`

### 5）批量快照不要自己手动切片

`get_quote()` 已经内置自动分批和多连接分发。除非你有特别明确的调度需求，否则直接把代码列表传进去即可。

### 6）所有时间字段都已经转成 Python 原生类型

- `trading_date`：`date`
- `time`：`datetime`
- `server_time`：`datetime | None`

## 文档导航

| 文档 | 作用 | 适合谁看 |
| --- | --- | --- |
| [`docs/README.md`](./docs/README.md) | 文档总览 | 想快速找到说明入口 |
| [`docs/API_REFERENCE.md`](./docs/API_REFERENCE.md) | 完整 API 用法 | 想看每个函数怎么调用 |
| [`docs/EXAMPLES.md`](./docs/EXAMPLES.md) | 示例集合 | 想直接抄可运行示例 |
| [`docs/FIELD_REFERENCE.md`](./docs/FIELD_REFERENCE.md) | 字段中文解释 | 想知道字段是什么含义 |
| [`docs/DEBUG_GUIDE.md`](./docs/DEBUG_GUIDE.md) | 调试指南 / smoke | 想定位问题或查看 raw 输出 |
| [`docs/maintainers/README.md`](./docs/maintainers/README.md) | 维护者资料 | 想看架构、验证记录、发布流程 |

## 项目结构

```text
eltdx/
├─ pyproject.toml
├─ README.md
├─ docs/
├─ scripts/
├─ tests/
└─ src/eltdx/
   ├─ client.py
   ├─ models.py
   ├─ protocol/
   ├─ transport/
   └─ services/
```

分层说明：

- `client.py`：对外主入口 `TdxClient`
- `models.py`：统一 dataclass 返回模型
- `protocol/`：协议编码、解码、字段恢复
- `transport/`：连接、读写、路由、心跳
- `services/`：围绕公开 API 的便捷封装
- `scripts/`：smoke、导出、验证脚本
- `tests/`：单元测试与集成测试

## FAQ

### Q1：`with TdxClient()` 会不会强制把长连接关掉？

不会强制所有人都这么用。

- 用 `with`：适合一次性任务，代码块结束自动关闭
- 手动 `connect()` / `close()`：适合长连接实时场景

### Q2：为什么 `get_count("sh")` 会这么大？

因为它是**代码表条目数**，不是股票总数。

### Q3：如果我只想拿 A 股代码，应该用哪个方法？

优先用：

- `get_a_share_codes_all()`

如果你希望把 B 股也算进去，再用：

- `get_stock_codes_all()`

### Q4：实时分时和历史分时是同一条协议吗？

不是。

当前实现里：

- `get_minute(code)`：实时分时路径
- `get_minute(code, date)` / `get_history_minute(code, date)`：历史分时路径

### Q5：为什么有些字段文档写得比较保守？

因为我们更重视**语义正确**，不希望把底层字段硬包装成看起来很确定、但其实证据不足的金融术语。

### Q6：如何核对字段的真实含义？

推荐顺序：

1. 看 `docs/FIELD_REFERENCE.md`
2. 再看 `docs/DEBUG_GUIDE.md`
3. 如需更深入的实现与验证资料，再看 `docs/maintainers/README.md`

### Q7：为什么仓库里保留了 `tests/` 和 `scripts/`？

因为它们属于仓库维护资源：

- `tests/` 用来保证协议解析和公开 API 行为稳定
- `scripts/` 用来做 smoke、导出和发布前验证
- 它们帮助维护质量，但不是对外公开 API

### Q8：这些测试和脚本会跟着 `pip install eltdx` 一起装进去吗？

正常安装主要使用打包后的库本体；日常使用重点是 `src/eltdx/` 下的代码。

## 路线图

### v0.1.x：首发可用版

- [x] 行情快照
- [x] 分时
- [x] 逐笔
- [x] K 线
- [x] 集合竞价
- [x] 公司行为 / 股本 / 复权因子
- [x] 自动分批
- [x] 双长连接
- [x] 上下文管理器
- [x] 在线验证与导出样本
- [x] API / 字段 / 调试文档

### v0.2.x：继续补强

- [ ] 扩大多周期 K 线在线抽检
- [ ] 扩大多品种样本验证
- [ ] 持续收敛少数字段的业务语义
- [ ] 补更多生产化示例

### 后续方向

- [ ] 发布流程进一步标准化
- [ ] 增加更多实盘核对样例
- [ ] 视需求评估更高层的数据服务能力

## 当前状态

- 当前版本处于 Alpha 阶段，主链路 API 已可用
- 公开说明集中在 API、示例、字段说明和调试四类文档
- 架构、验证记录和发布流程见：[`docs/maintainers/README.md`](./docs/maintainers/README.md)

## 许可证

MIT
