<div align="center">
  <h1>eltdx</h1>
  <p><strong>通达信行情协议的 Python 库</strong></p>
  <p>安装后就能直接调快照、分时、逐笔、K 线、集合竞价等接口，返回结果统一、字段清楚，也支持查看原始十六进制数据。主要用于个人行情研究，请勿用于任何商业或违法用途。</p>
  <p>
    <a href="https://pypi.org/project/eltdx/"><img alt="PyPI" src="https://img.shields.io/pypi/v/eltdx?cacheSeconds=300&logo=pypi"></a>
    <a href="https://pypi.org/project/eltdx/"><img alt="Python 3.10+" src="https://img.shields.io/badge/Python-3.10%2B-blue"></a>
    <a href="https://github.com/electkismet/eltdx/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/electkismet/eltdx/ci.yml?branch=main"></a>
    <a href="https://github.com/electkismet/eltdx/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/pypi/l/eltdx"></a>
  </p>
</div>

## 简单介绍

`eltdx` 优点：

- 一个统一入口：`TdxClient`
- 让常用接口直接就能调：`get_quote()`、`get_minute()`、`get_trades()`、`get_kline()` 等
- 返回结果尽量好懂：统一 dataclass，不直接丢一堆裸 `dict`
- 时间字段给 Python 原生 `date` / `datetime`
- 价格字段同时保留浮点值和 `*_milli` 整数值
- 需要排查问题时，可以打开 `include_raw=True` 看原始十六进制

当前版本重点覆盖：

- 行情快照
- 分时
- 逐笔
- K 线
- 集合竞价
- 公司行为 / 股本变化 / 复权因子
- 一些常用 helper

## 安装

推荐的安装方式：

```bash
python -m pip install eltdx
```

升级：

```bash
python -m pip install -U eltdx
```

如果你是拉源码自己调：

```bash
pip install -e .
```

环境要求：

- Python `>=3.10`

## 快速开始

最简单的用法：

```python
from eltdx import TdxClient

with TdxClient() as client:
    quotes = client.get_quote(["sz000001", "sh600000"])
    for quote in quotes:
        print(quote.code, quote.last_price, quote.server_time)
```

如果你想看原始协议数据：

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_minute("sz000001", include_raw=True)
    print(minute.raw_frame_hex)
    print(minute.raw_payload_hex)
```

如果你想自己指定服务器地址：

```python
from eltdx import TdxClient

with TdxClient(
    hosts=["124.71.187.122:7709", "122.51.120.217:7709"],
    pool_size=2,
    batch_size=80,
) as client:
    quote = client.get_quote("sz000001")[0]
    print(quote.last_price)
```

## 常用功能

### 先连上，再取数据

- `connect()`：主动建立连接
- `close()`：关闭连接
- `with TdxClient() as client:`：适合一次性脚本，跑完自动关

### 最常会用到的接口

- `get_quote()`：拿快照行情
- `get_minute()` / `get_history_minute()`：拿分时
- `get_trades()` / `get_trades_all()`：拿逐笔
- `get_kline()` / `get_kline_all()`：拿 K 线
- `get_call_auction()`：拿集合竞价
- `get_gbbq()` / `get_xdxr()` / `get_equity()` / `get_factors()`：拿公司行为、股本和复权相关数据

### 用起来比较省心的地方

- `with` 可用可不用；短任务推荐用，长连接也可以手动控制
- `get_quote()` 自带自动分批，不用你自己切列表
- 默认支持多连接分发，批量取快照会更稳一些
- 返回字段统一，做展示、落库、计算都比较顺手

## 注意

### `get_count(exchange)` 不是股票总数

它是“这个市场的代码表条目数”，不是“这个市场一共有多少只股票”。

如果你更想拿到股票口径的数量，可以试试：

- `get_stock_count(exchange)`
- `get_a_share_count(exchange)`

### `get_codes()` / `get_codes_all()` 不只是股票列表

这里面会混有股票、指数、板块分类项、ETF、基金、债券回购等条目。

如果你想直接拿一组更实用的代码去拉行情，可以试试：

- `get_stock_codes_all()`
- `get_a_share_codes_all()`
- `get_etf_codes_all()`
- `get_index_codes_all()`

### `with` 不是强制的

- 你只是临时拉一把数据：用 `with`
- 你要一直保持连接：手动 `connect()` / `close()` 也可以

### `host` 和 `hosts` 都可以传

- `host=`：只测一个地址时比较方便
- `hosts=`：传多个地址，交给客户端自己处理
- 不传时会回退到库内默认地址列表

## 文档指北

如果你是第一次用，建议按这个顺序看：

- [文档首页](https://github.com/electkismet/eltdx/blob/main/docs/README.md)
- [API 用法](https://github.com/electkismet/eltdx/blob/main/docs/API_REFERENCE.md)
- [使用示例](https://github.com/electkismet/eltdx/blob/main/docs/EXAMPLES.md)
- [字段说明](https://github.com/electkismet/eltdx/blob/main/docs/FIELD_REFERENCE.md)
- [调试指南](https://github.com/electkismet/eltdx/blob/main/docs/DEBUG_GUIDE.md)

## FAQ

### 如果我只拉一次数据，推荐怎么写？

直接用：

```python
with TdxClient() as client:
    ...
```

这样最省心，代码块结束后会自动关连接。

### 如果我要做长连接实时场景呢？

那就自己控制连接生命周期：

```python
client = TdxClient()
client.connect()
try:
    ...
finally:
    client.close()
```

### 为什么 `get_count("sh")` 看起来特别大？

因为它不是股票总数，而是代码表条目数。

### 为什么 `get_codes()` 里看起来不全是股票？

因为它本来就不只是股票，还会带上指数、ETF、板块分类项等条目。

### 我只想拿 A 股代码，应该用哪个？

优先用：

- `get_a_share_codes_all()`

如果你希望把 B 股也一起算进去，再试：

- `get_stock_codes_all()`

## 项目参考

- 感谢作者[injoyai/tdx](https://github.com/injoyai/tdx)


## 联系方式

- QQ 群：复盘软件交流群（群号：`982691484`）
- 加群链接：[点击链接加入群聊](https://qm.qq.com/q/zAjpZsvfzy)
- 邮箱：`dapaoxixixi@163.com`

## 许可证

MIT
