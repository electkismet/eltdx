<div align="center">
  <h1>eltdx</h1>
  <p><strong>通达信行情协议的 Python 客户端库</strong></p>
  <p>提供统一入口、明确方法名、稳定 dataclass 返回模型，以及可选原始十六进制调试能力。</p>
  <p>
    <a href="https://pypi.org/project/eltdx/"><img alt="PyPI" src="https://img.shields.io/pypi/v/eltdx"></a>
    <a href="https://pypi.org/project/eltdx/"><img alt="Python Versions" src="https://img.shields.io/pypi/pyversions/eltdx"></a>
    <a href="https://github.com/electkismet/eltdx/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/electkismet/eltdx/ci.yml?branch=main"></a>
    <a href="https://github.com/electkismet/eltdx/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/pypi/l/eltdx"></a>
  </p>
</div>

## 简介

`eltdx` 面向 Python 使用场景，目标是让行情接口调用尽量简单：

- 一个主入口：`TdxClient`
- 多个方法名：`get_quote()`、`get_minute()`、`get_trades()`、`get_kline()` 等
- 统一返回 dataclass：不直接向外暴露裸 `dict`
- 所有时间字段统一转换为 Python 原生 `date` / `datetime`
- 价格字段同时保留浮点值和 `*_milli` 整数值
- 关键协议支持 `include_raw=True`，可查看原始 `frame/payload` 十六进制

当前版本重点覆盖：

- 行情快照
- 分时
- 逐笔
- K 线
- 集合竞价
- 公司行为 / 股本变化 / 复权因子
- 常用 helper 与服务层封装

## 项目参考
- [injoyai](https://github.com/injoyai/tdx)

## 安装

### 推荐安装

```bash
python -m pip install eltdx
```

### 升级

```bash
python -m pip install -U eltdx
```

### 本地开发安装

```bash
pip install -e .
```

环境要求：

- Python `>=3.10`

## 30 秒上手

```python
from eltdx import TdxClient

with TdxClient() as client:
    quotes = client.get_quote(["sz000001", "sh600000"])
    for quote in quotes:
        print(quote.code, quote.last_price, quote.server_time)
```

查看原始协议数据：

```python
from eltdx import TdxClient

with TdxClient() as client:
    minute = client.get_minute("sz000001", include_raw=True)
    print(minute.raw_frame_hex)
    print(minute.raw_payload_hex)
```

自定义服务器地址：

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

## 为什么用它

- `with TdxClient()` 支持上下文管理器，适合一次性抓取任务
- 也支持手动 `connect()` / `close()`，适合长连接实时场景
- `get_quote()` 内置自动分批，默认每批 `80` 个代码，并默认使用两条连接分发请求
- 返回模型统一，时间字段和价格字段都已经做过 Python 化处理
- 可以在调试阶段直接拿到原始十六进制数据做人工核对

## API 概览

### 生命周期

- `connect()`：主动建立连接池
- `close()`：关闭连接池
- `with TdxClient() as client:`：适合一次性脚本和导出任务

### 行情与代码表

- `get_quote()`：快照行情
- `get_count()` / `get_codes()` / `get_codes_all()`：底层代码表能力
- `get_stock_count()` / `get_a_share_count()`：过滤后的股票数量 helper
- `get_stock_codes_all()` / `get_a_share_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()`：常用代码筛选 helper

### 时间序列

- `get_minute()` / `get_history_minute()`：分时
- `get_trades()` / `get_trades_all()`：逐笔
- `get_kline()` / `get_kline_all()`：K 线
- `get_call_auction()`：集合竞价

### 公司行为与复权

- `get_gbbq()`：公司行为原始记录
- `get_xdxr()`：除权除息记录
- `get_equity_changes()` / `get_equity()`：股本变化与指定日期股本
- `get_factors()`：复权因子
- `get_adjusted_kline()` / `get_adjusted_kline_all()`：复权 K 线

### 派生 helper

- `get_turnover()`：换手率 helper
- `get_trade_minute_kline()` / `get_history_trade_minute_kline()`：逐笔聚合分钟 K 线

完整参数、返回值和示例见完整文档。

## 重要说明

### `get_count(exchange)` 不是股票总数

它返回的是底层代码表条目数，不是“这个市场一共有多少只股票”。

如果你要更接近股票口径的数量，请优先使用：

- `get_stock_count(exchange)`
- `get_a_share_count(exchange)`

### `get_codes*()` 不是官方证券分类总表

代码表里会混有股票、指数、板块分类项、ETF、基金、债券回购等条目。

如果你想直接拿一组更实用的代码去拉行情，优先使用：

- `get_stock_codes_all()`
- `get_a_share_codes_all()`
- `get_etf_codes_all()`
- `get_index_codes_all()`

### `with` 关闭是可选的

- 一次性拉数据：推荐 `with`
- 长连接实时场景：可以手动持有 `client`，最后再 `close()`

### `host` 和 `hosts` 都可以传

- `host=`：指定一个地址
- `hosts=`：传多个地址给客户端自动处理
- 不传时会回退到库内默认地址列表

## 文档

完整文档放在 GitHub：

- [文档首页](https://github.com/electkismet/eltdx/blob/main/docs/README.md)
- [API Reference](https://github.com/electkismet/eltdx/blob/main/docs/API_REFERENCE.md)
- [Examples](https://github.com/electkismet/eltdx/blob/main/docs/EXAMPLES.md)
- [Field Reference](https://github.com/electkismet/eltdx/blob/main/docs/FIELD_REFERENCE.md)
- [Debug Guide](https://github.com/electkismet/eltdx/blob/main/docs/DEBUG_GUIDE.md)
- [Maintainers](https://github.com/electkismet/eltdx/blob/main/docs/maintainers/README.md)

## FAQ

### 关于仓库里的 `tests/` 和 `scripts/`说明

它们属于仓库维护资源：

- `tests/` 用来保证协议解析和公开 API 行为稳定
- `scripts/` 用来做 smoke、导出和发布前验证
- 它们帮助维护质量，但不是对外公开 API

### 这些测试和脚本会跟着 `pip install eltdx` 一起装进去吗？

日常安装重点使用的是库本体；普通使用场景只需要关注 `eltdx` 包本身。

## 当前状态

- 当前版本：`0.1.0`
- 发布地址：[PyPI / eltdx](https://pypi.org/project/eltdx/)
- 仓库地址：[GitHub / electkismet/eltdx](https://github.com/electkismet/eltdx)
- 当前定位：Alpha，可用于实际拉取和二次开发验证

## 许可证

MIT
