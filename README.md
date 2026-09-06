<h1 align="center">eltdx</h1>

<p align="center">
  <strong>Python API，Rust 驱动的通达信量化数据客户端，支持其他语言通过HTTP或WebSocket调用</strong>
</p>

<p align="center">
  <a href="https://electkismet.github.io/eltdx/"><strong>接口一览</strong></a>
  ·
  <a href="https://pypi.org/project/eltdx/">PyPI</a>
</p>

<p align="center">
  <a href="https://github.com/electkismet/eltdx/actions/workflows/ci.yml"><img alt="构建状态" src="https://img.shields.io/github/actions/workflow/status/electkismet/eltdx/ci.yml?branch=main&amp;label=%E6%A3%80%E6%9F%A5"></a>
  <a href="https://pypi.org/project/eltdx/"><img alt="PyPI eltdx" src="https://img.shields.io/pypi/v/eltdx?label=PyPI&amp;color=0969da"></a>
  <a href="https://electkismet.github.io/eltdx/"><img alt="文档站" src="https://img.shields.io/github/actions/workflow/status/electkismet/eltdx/pages.yml?branch=main&amp;label=%E6%96%87%E6%A1%A3"></a>
  <img alt="Python 3.10+" src="https://img.shields.io/badge/Python-3.10%2B-3776AB?logo=python&amp;logoColor=white">
  <a href="./LICENSE"><img alt="Research-Only License" src="https://img.shields.io/badge/协议-Research--Only-6f42c1"></a>
</p>

<a href="https://electkismet.github.io/eltdx/">
  <img alt="eltdx 接口目录预览" src=".github/assets/eltdx-readme-banner-v3.0.5.png">
</a>

<p align="center">
  <a href="https://api.astlane.com/">
    <img alt="Astlane token 赞助方" src="docs/assets/astlane-sponsor.svg" width="860">
  </a>
</p>

> 如果需要多数据源可以关注新项目 [AxData](https://github.com/electkismet/AxData)：AxData 基于 eltdx 迭代开发，除通达信体系外，AxData 还通过插件机制整理接入交易所、巨潮、腾讯财经、新浪财经、东方财富、财联社、开盘红等公开源接口，并扩展了自由流通市值、开盘换手、开盘量比、开盘抢筹、竞价昨比、连板天梯、题材强度等更适合本地量化研究和短线数据分析的指标能力，但如果是用单一源的话，仍推荐eltdx，受制于架构axdata性能不如裸协议路径的eltdx。

通达信在线行情协议 Python 库。可以拿 A 股的行情、分时、成交明细、K 线、竞价、公司信息、题材信息等信息，支持 MCP 工具。`3.0` 保留现有 Python API 和返回模型，将 21 个 7709 命令的构包、解析、连接池、pin、push、心跳和关闭核心统一迁入 Rust。

> `v2.0.0` 移除 `TdxClient` 的旧版扁平旧版入口，统一使用模块化 API 和 Helpers。升级前请阅读 [v2.0.0 迁移说明](docs/releases/v2.0.0.md)。

1. 本项目仅以个人学习、协议研究和非商业研究为目的进行开发。
2. 本项目基于互联网公开信息搜集开发。
3. 项目本身、衍生产品及通过本项目获取的数据禁止用于任何商业行为、付费服务、生产服务、转售或其他营利用途，产生的任何数据、损失或法律责任由使用者自负。
4. 对第三方服务器或服务的访问，用户需自行遵守相关法律法规及服务协议。
5. 请勿将本项目用于侵犯他人权益、违反监管规定或滥用第三方服务的行为。

感谢 [injoyai/tdx](https://github.com/injoyai/tdx) 及 [rainx/pytdx](https://github.com/rainx/pytdx) 的启发。

## 功能地图

eltdx 默认按“想拿什么数据”组织入口。普通调用优先使用模块化 API 和 Helpers；只有做协议研究时，才需要直接对照 `7709` 命令或 `7615` Entry。下表的代表调用均来自当前接口文档，点击方法名可直接查看参数、返回字段和同类方法。

<p align="center">
  <strong>Python / MCP</strong>
  &nbsp;→&nbsp;
  <strong>模块化 API / Helpers</strong>
  &nbsp;→&nbsp;
  <strong>7709 原生协议 / 7615 原生 Entry</strong>
  &nbsp;→&nbsp;
  <strong>dataclass / push / JSON</strong>
</p>

| 研究任务 | 可以直接拿到 | 代表调用（点开文档） | 数据路径 |
| --- | --- | --- | --- |
| A 股清单 | 最新股票、ST、停牌列表和代码表 | [`client.helpers.latest_stock_list()`](docs/helpers/A股常用封装.md) | `7709` + `Helpers 封装` |
| 分类行情 | 按市场或板块分页、排序的行情列表 | [`client.quotes.list_by_category()`](docs/methods/7709-分类行情.md) | `7709 原生协议` |
| 完整实时行情 | 批量快照、最新价、成交量额和完整五档 | [`client.helpers.full_quotes()`](docs/helpers/完整行情.md) | `7709` + `Helpers 封装` |
| 五档实时刷新 | 为代码列表建立或刷新实时五档 | [`client.quotes.get_depth()`](docs/methods/7709-增量刷新推送队列.md) | `7709 原生协议` |
| push 队列 | 读取未匹配的实时更新帧 | [`client.quotes.poll_push()`](docs/methods/7709-增量刷新推送队列.md) | `7709 原生协议` |
| K 线与复权 | 分钟/日/周/月/季/年 K 线、自动分页、前/后/定点复权 | [`client.bars.get()`](docs/methods/7709-K线周期线.md) | `7709 原生协议` |
| 当日分时 | 当日每分钟价格、成交量和均价 | [`client.minutes.today()`](docs/methods/7709-当日分时.md) | `7709 原生协议` |
| 当日成交明细 | 自动分页合并的当日完整成交记录 | [`client.trades.all_today()`](docs/methods/7709-当日成交明细.md) | `7709 原生协议` |
| 集合竞价 | 竞价过程、09:25 撮合、前收盘参考价、开盘价/量/额/涨幅 | [`client.helpers.auction_data()`](docs/helpers/竞价数据.md) | `7709` + `Helpers 封装` |
| 股本变迁 | 除权除息、股本变化、增发和回购记录 | [`client.corporate.capital_changes()`](docs/methods/7709-股本变迁GBBQ.md) | `7709 原生协议` |
| 财务基础 | 流通/总股本、EPS、资产、负债、收入和利润 | [`client.corporate.finance_batch()`](docs/methods/7709-财务基础信息.md) | `7709 原生协议` |
| 本地复权系数 | 为已有本地不复权 K 线提供前/后复权所需系数 | [`client.corporate.adjustment_factors()`](docs/methods/7709-本地复权系数.md) | `7709 本地计算` |
| 特殊品种限制 | 特殊品种涨跌停限制表 | [`client.limits.special()`](docs/methods/7709-特殊品种涨跌停限制.md) | `7709 原生协议` |
| F10 公司概况 | 发行上市信息、上市日期、发行价、募资额和承销商 | [`client.f10.company_profile()`](docs/methods/F10-公司概况.md) | `7615 原生 Entry` |
| 个股概念 | 某只股票所属的热点题材和入选原因 | [`client.helpers.stock_topics()`](docs/helpers/个股概念板块.md) | `7615` + `Helpers 封装` |
| 股票信息汇总 | 行情、证券名称、股本、市值和换手率合并表 | [`client.helpers.stock_profile_table()`](docs/helpers/股票信息汇总.md) | `7709` + `Helpers 组合` |
| 短线指标 | 竞价、开盘量比、流通股本、封单和几天几板等指标 | [`client.helpers.shortline_indicators()`](docs/helpers/短线指标.md) | `7709` + `Helpers 组合` |
| A 股常用榜单 | 每日股本、涨跌停价、实时榜单、连板天梯和题材强度 | [`client.helpers.realtime_rank()`](docs/helpers/A股常用封装.md) | `7709` + `Helpers 组合` |
| 服务器文件 | 按分块自动下载并合并完整文件 | [`client.resources.download_file()`](docs/methods/7709-服务器文件下载.md) | `7709 原生协议` |
| 连接与并发 | 主站测速、Rust 连接池、心跳、pin、push、缓存和 diagnostics | [`TdxClient()`](docs/METHOD_REFERENCE.md#tdxclient) | `Rust Engine` |
| MCP 工具服务 | 将常用行情、F10、竞价和文档能力提供给 MCP 客户端 | [`eltdx-mcp`](docs/MCP.md#安装和启动) | `MCP` |
| 跨语言网关 | 通过 HTTP JSON、WebSocket RPC 和实时订阅供其他语言调用 | [`eltdx-http`](docs/HTTP_GATEWAY.md) | `FastAPI` |

完整接口目录见 [GitHub Pages](https://electkismet.github.io/eltdx/)。调用方法和返回字段看 [METHOD_REFERENCE.md](docs/METHOD_REFERENCE.md)，常用问题入口看 [docs/helpers/README.md](docs/helpers/README.md)，完整 API 看 [API_REFERENCE.md](docs/API_REFERENCE.md)，字段总表看 [FIELD_REFERENCE.md](docs/FIELD_REFERENCE.md)，F10 资料看 [F10_7615.md](docs/F10_7615.md)，MCP 工具看 [MCP.md](docs/MCP.md)。

## 安装

```bash
pip install eltdx
```

`3.0` 为 CPython 3.10-3.14 提供 Windows x64、manylinux x64/ARM64、macOS x64/ARM64 五个 `cp310-abi3` wheel。匹配 wheel 时不需要安装 Rust。没有匹配 wheel 的平台会尝试从 sdist 编译，需要 Rust 1.89 工具链；3.0 不提供纯 Python 7709 fallback。PyPy、free-threaded CPython、musllinux 和其他未列出的组合不在首发支持范围。

如果需要启动 MCP stdio 工具服务，安装可选依赖：

```bash
pip install "eltdx[mcp]"
```

Java、Go、C#、Node.js 等语言需要通过 HTTP 或 WebSocket 调用时，再安装网关依赖：

```bash
pip install "eltdx[http]"
eltdx-http
```

默认 `pip install eltdx` 不会安装 FastAPI 或 Uvicorn，也不会启动额外服务。网关用法见 [跨语言 HTTP / WebSocket 网关](docs/HTTP_GATEWAY.md)。

源码目录安装：

```bash
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -U pip
pip install -e ".[dev,mcp]"
```

源码安装会编译私有扩展 `eltdx._native`。Python 文件与 native 二进制的 ABI 不一致时，导入会立即失败，不会静默切换到旧实现。

源码开发时建议始终先安装到当前虚拟环境；否则本机如果已有旧版 `eltdx`，`python -m eltdx...` 可能导入 site-packages 里的旧包。

安装后可以先看命令帮助：

```bash
eltdx-smoke --help
eltdx-f10-smoke --help
```

MCP 工具服务启动后会占用当前终端作为 stdio 服务：

```bash
eltdx-mcp
```

源码开发自测：

```bash
python -m pytest
```

源码仓库里的 `scripts/` 目录用于开发和排查；通过 `pip install eltdx` 安装后，优先使用上面的命令行入口。

## 快速开始

查行情走 `TdxClient`

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    quote = client.quotes.get_snapshots(["sz000001", "sh600000"])
    bars = client.bars.get("sz000001", period="day", count=30)
    minute = client.minutes.today("sz000001")
    ticks = client.trades.all_history("sz000001", "2026-05-20")

print(quote[0])
print(bars.bars[-1])
```

查 F10 / 资料数据走 `client.f10`

```python
from eltdx import TdxClient

client = TdxClient(timeout=3)
profile = client.f10.company_profile("000034")
topics = client.f10.hot_topics("000034")
notices = client.f10.announcements("000034")

print(profile.rows[0])
print(topics.rows[:3])
print(notices.rows[:3])
```

如果只查 F10，也可以直接用轻量 HTTP 客户端：

```python
from eltdx import F10Client

f10 = F10Client(timeout=3)
print(f10.company_profile("000034").rows[0])
```

## 行情接口

| 功能           | 调用方法                                                       | 底层接口                                                                                        | 返回内容 / 用途                                                      | 文档                                    |
| ------------ | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------- | ------------------------------------- |
| 握手           | `client.session.handshake()`                               | [`0x000d`](docs/COMMANDS_7709.md#cmd-0x000d)                                                | 返回服务端日期时间、交易时段、主站名、产品标识；通常连接后自动使用                              | [文档](docs/methods/7709-握手.md)         |
| 心跳           | `client.session.heartbeat()`                               | [`0x0004`](docs/COMMANDS_7709.md#cmd-0x0004)                                                | 返回服务端心跳响应；长连接默认后台 30 秒保活，也可手动调用                                | [文档](docs/methods/7709-心跳.md)         |
| 市场代码数量       | `client.codes.count(market)` / `client.codes.a_share_count(market)`                 | [`0x044e`](docs/COMMANDS_7709.md#cmd-0x044e) / [`0x044d`](docs/COMMANDS_7709.md#cmd-0x044d) | 分别返回整个市场代码表条数或仅 A 股数量；前者不限证券品种                              | [文档](docs/methods/7709-代码数量.md)       |
| 代码表          | `client.codes.all(market, ...)` / `client.codes.all_a_shares()` / `client.codes.list(market, ...)` | [`0x044d`](docs/COMMANDS_7709.md#cmd-0x044d)                                                | 推荐自动翻页取全量；也可直接取 A 股、ETF、指数，或手动控制单页                         | [文档](docs/methods/7709-代码表.md)        |
| 批量行情快照       | `client.quotes.get_snapshots()`            | [`0x054c`](docs/COMMANDS_7709.md#cmd-0x054c) | 原生一次性基础快照，返回现价、成交量额和已确认的一档盘口                   | [文档](docs/methods/7709-批量快照.md)       |
| 完整行情 / 五档盘口 | `client.helpers.full_quotes()` | [`0x054c`](docs/COMMANDS_7709.md#cmd-0x054c) + [`0x0547`](docs/COMMANDS_7709.md#cmd-0x0547) | 普通用户推荐入口，自动组合基础快照与实时五档 | [文档](docs/helpers/完整行情.md) |
| 原生批量行情       | `client.quotes.legacy()`                              | [`0x053e`](docs/COMMANDS_7709.md#cmd-0x053e)                                                | 原生旧版完整快照，保留五档盘口和协议状态原始字段                    | [文档](docs/methods/7709-旧版批量行情.md)     |
| 五档刷新 / 推送队列 | `client.quotes.get_depth()` / `client.quotes.refresh()` / `client.quotes.poll_push()` | [`0x0547`](docs/COMMANDS_7709.md#cmd-0x0547) | 原生五档快捷入口、游标刷新与推送队列；面向高级实时更新场景 | [文档](docs/methods/7709-增量刷新推送队列.md) |
| 分类行情         | `client.quotes.list_by_category()`                         | [`0x054b`](docs/COMMANDS_7709.md#cmd-0x054b)                                                | 按市场或板块分页返回行情列表；可按涨幅、价格、成交额等服务端排序                               | [文档](docs/methods/7709-分类行情.md)       |
| K 线 / 周期线    | `client.bars.get(code_or_codes, ..., all_pages=False)` | [`0x052d`](docs/COMMANDS_7709.md#cmd-0x052d) | 单页或自动分页返回 K 线；支持单只或多只证券并发查询，以及分钟、日、周、月、季、年线和服务端复权参数 | [文档](docs/methods/7709-K线周期线.md) |
| 当日分时         | `client.minutes.today()`                  | [`0x0537`](docs/COMMANDS_7709.md#cmd-0x0537)                                                | 返回主站当前保存的每分钟价格、成交量、均价等分时序列                                       | [文档](docs/methods/7709-当日分时.md)       |
| 指定日期历史分时     | `client.minutes.history()`        | [`0x0fb4`](docs/COMMANDS_7709.md#cmd-0x0fb4)                                                | 按日期返回某天的分时价格和分钟成交量，适合补单日历史分时                                   | [文档](docs/methods/7709-指定日期历史分时.md)   |
| 近期历史分时       | `client.minutes.recent()`                                  | [`0x0feb`](docs/COMMANDS_7709.md#cmd-0x0feb)                                                | 返回服务端近期窗口内的历史分时；适合查较近交易日的分钟走势                                  | [文档](docs/methods/7709-近期历史分时.md)     |
| 分时副图         | `client.minutes.aux()`                                     | [`0x051b`](docs/COMMANDS_7709.md#cmd-0x051b)                                                | 返回分时页下方副图数据，例如买卖力道、成交对比等序列                                     | [文档](docs/methods/7709-分时副图.md)       |
| 小走势图         | `client.minutes.sparkline()`                               | [`0x0fd1`](docs/COMMANDS_7709.md#cmd-0x0fd1)                                                | 返回单标的小型价格走势序列，适合列表页或概览页的小图                                     | [文档](docs/methods/7709-小走势图.md)       |
| 当日成交明细       | `client.trades.today(code, ...)` / `client.trades.all_today(code, ...)`                   | [`0x0fc5`](docs/COMMANDS_7709.md#cmd-0x0fc5)                                                | `ticks` 保留原始混合记录；`actual_trades` 排除 `status=8` 竞价快照，并保留 09:25、15:00 和 `status=5` 盘后真实成交 | [文档](docs/methods/7709-当日成交明细.md)     |
| 当日 09:25 正式撮合 | `client.trades.opening_match_today(code, ...)` | [`0x0fc5`](docs/COMMANDS_7709.md#cmd-0x0fc5) | 从当日成交明细筛选 09:25 正式撮合；没有记录时返回 `None` | [文档](docs/methods/7709-当日0925正式撮合.md) |
| 历史成交明细       | `client.trades.history(code, date, ...)` / `client.trades.all_history(code, date, ...)`      | [`0x0fc6`](docs/COMMANDS_7709.md#cmd-0x0fc6)                                                | `history()` 返回一页，`all_history()` 自动翻页；用 `actual_trades` 读取排除竞价快照后的真实成交 | [文档](docs/methods/7709-历史成交明细.md)     |
| 历史 09:25 正式撮合 | `client.trades.opening_match_history(code, date, ...)` | [`0x0fc6`](docs/COMMANDS_7709.md#cmd-0x0fc6) | 从历史成交明细筛选指定日期 09:25 正式撮合；没有记录时返回 `None` | [文档](docs/methods/7709-历史0925正式撮合.md) |
| 集合竞价过程快照 | `client.auctions.series(code, date=None)`          | [`0x056a`](docs/COMMANDS_7709.md#cmd-0x056a)                                                | 不传日期查询当日，传入日期查询历史；按主站顺序返回全部过程时间点，即使出现 `09:25` 也不是正式成交                       | [文档](docs/methods/7709-集合竞价明细.md)     |
| 股本变迁 / GBBQ  | `client.corporate.capital_changes()` | [`0x000f`](docs/COMMANDS_7709.md#cmd-0x000f) | 返回标签 `1..15` 的广义权息和股本事件记录 | [文档](docs/methods/7709-股本变迁GBBQ.md) |
| 本地复权系数 | `client.corporate.adjustment_factors()` | [`0x000f`](docs/COMMANDS_7709.md#cmd-0x000f) | 为已有本地不复权 K 线计算事件级前/后复权仿射系数 | [文档](docs/methods/7709-本地复权系数.md) |
| 财务基础信息       | `client.corporate.finance_batch(codes)` | [`0x0010`](docs/COMMANDS_7709.md#cmd-0x0010)                                                | 批量返回流通股本、总股本、EPS、资产、负债、收入、利润等基础财务字段                            | [文档](docs/methods/7709-财务基础信息.md)     |
| 特殊品种涨跌停限制    | `client.limits.special()`                                  | [`0x0452`](docs/COMMANDS_7709.md#cmd-0x0452)                                                | 返回特殊品种涨跌停限制表；需要按表扫描后本地索引到具体代码                                  | [文档](docs/methods/7709-特殊品种涨跌停限制.md)  |
| 服务器文件分块读取      | `client.resources.read()` | [`0x06b9`](docs/COMMANDS_7709.md#cmd-0x06b9) | 读取一个服务器文件块，返回原始 bytes 和长度头 | [文档](docs/methods/7709-服务器文件读取.md)    |
| 服务器文件下载      | `client.resources.download_file()` | [`0x06b9`](docs/COMMANDS_7709.md#cmd-0x06b9) | 循环读取并合并完整服务器文件 | [文档](docs/methods/7709-服务器文件下载.md)    |
| 服务器统计文件解析      | `client.resources.read_stats()` | [`0x06b9`](docs/COMMANDS_7709.md#cmd-0x06b9) | 下载并解析 `zhb.zip` 中的统计文件 | [文档](docs/methods/7709-服务器统计文件解析.md)    |
| 短线指标（Helper） | `client.helpers.shortline_indicators()`                    | `0x06b9 + 0x054c + 0x0547 + 0x044d + 0x052d + 0x0010`                                    | 按交易日对齐统计资源、实时行情、近 5 日 K 线和财务快照，返回竞价、开盘量比、流通股本、封单和连板指标 | [文档](docs/helpers/短线指标.md)              |

`7709` 命令和 API 对照见 [COMMANDS_7709.md](docs/COMMANDS_7709.md)，完整调用参数见 [API_REFERENCE.md](docs/API_REFERENCE.md)。

### K 线周期和复权

K 线是最常用的接口之一，周期和复权参数可以直接这样传：

```python
client.bars.get("sz000001", period="day", count=200)
client.bars.get("sz000001", period="week", count=100)
client.bars.get("sz000001", period="year", count=20)
client.bars.get("sz000001", period="1m", count=240)
client.bars.get("sz000001", period="day", adjust="qfq", count=200)
client.bars.get("sz000001", period="day", adjust="fixed_qfq", anchor_date="2024-06-03")
client.bars.get("sz000001", period="day", all_pages=True, page_size=800)
```

| 参数            | 可选值                                       | 含义                                |
| ------------- | ----------------------------------------- | --------------------------------- |
| `period`      | `1m`, `5m`, `15m`, `30m`, `60m`           | 分钟 K 线                            |
| `period`      | `day`, `week`, `month`, `quarter`, `year` | 日 K、周 K、月 K、季 K、年 K               |
| `period`      | `10m`, `2d`, `5s` 这类形式                    | 协议层支持自定义分钟、N 日、N 秒周期；实际返回以服务端支持为准 |
| `adjust`      | `None` / `none`                           | 不复权                               |
| `adjust`      | `qfq` / `front`                           | 前复权                               |
| `adjust`      | `hfq` / `back`                            | 后复权                               |
| `adjust`      | `fixed_qfq` / `fixed_hfq`                 | 定点前复权 / 定点后复权，需要配合 `anchor_date`  |
| `anchor_date` | `YYYY-MM-DD`、`YYYYMMDD`、`date`            | 定点复权基准日期，仅定点复权时需要                 |

## F10 资料接口

| 功能           | 调用方法                                                    | 底层 Entry                                                                              | 返回内容 / 用途                                    | 文档                                  |
| ------------ | ------------------------------------------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------- | ----------------------------------- |
| 高级调用（需手动指定 Entry 和参数） | `client.f10.call(entry, body/params)`                   | [`7615/TQLEX`](docs/F10_7615.md#tqlex-gateway)                                        | 用户主动指定 Entry 和参数；用于调试或调用 SDK 尚未封装的 Entry，不会自动执行 | [文档](docs/methods/F10-通用Entry调用.md) |
| 股票基础信息       | `client.f10.stock_info()`                               | [`CWServ.tdxf10_gg_comreq`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-comreq)           | 返回股票名称、代码、市场；也用于主营构成报告期、题材 ID 等辅助查询          | [文档](docs/methods/F10-股票基础信息.md)    |
| 涨跌停列表         | `client.f10.limit_up_down_list()`                       | [`CWServ.cfg_fx_lbtt`](docs/F10_7615.md#entry-cwserv-cfg-fx-lbtt)                     | 返回指定日期 / 日期范围的逐股涨停、连板、涨停原因和封单明细；可选市场概况   | [文档](docs/methods/F10-涨跌停列表.md)       |
| 公司概况         | `client.f10.company_profile()`                          | [`CWServ.tdxf10_gg_gsgk`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-gsgk)               | 返回发行上市信息，如上市日期、发行方式、发行价、募资额、承销商等             | [文档](docs/methods/F10-公司概况.md)      |
| 主营构成         | `client.f10.business_composition()`                     | [`CWServ.tdxf10_gg_jyfx`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-jyfx)               | 返回主营收入、成本、毛利、收入占比、毛利率；不传报告期时自动取最新期           | [文档](docs/methods/F10-主营构成.md)      |
| 股东增减持        | `client.f10.shareholder_change_plans()`                 | [`CWServ.tdxf10_gg_gdyj`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-gdyj)               | 返回公告日、股东名称、变动方向、拟变动数量 / 比例、计划起止日期等           | [文档](docs/methods/F10-股东增减持.md)     |
| 分红融资         | `client.f10.dividend_financing()`                       | [`CWServ.tdxf10_gg_fhrz`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-fhrz)               | 返回分红方案、股权登记日、除权派息日、股息率、股利支付率、融资相关数据          | [文档](docs/methods/F10-分红融资.md)      |
| 增发获配         | `client.f10.allotment_dates()` / `allotment_details()`  | [`CWServ.tdxf10_gg_fhrz_zfhpmx`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-fhrz-zfhpmx) | 先取增发日期，再按日期取获配机构、获配数量、获配金额、锁定期等明细            | [文档](docs/methods/F10-增发获配.md)      |
| 财务报表         | `client.f10.finance_report()`                           | [`CWServ.tdxf10_gg_cwfx`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-cwfx)               | 返回多期财务报表；默认资产负债表，含货币资金、资产总计、负债合计、股东权益等       | [文档](docs/methods/F10-财务报表.md)      |
| 财务诊断         | `client.f10.finance_diagnosis()`                        | [`CWServ.tdxf10_gg_cwzd`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-cwzd)               | 返回营运、盈利、成长、现金流、资产质量、Z 值预警、财务总评分等诊断项          | [文档](docs/methods/F10-财务诊断.md)      |
| 个股总评         | `client.f10.stock_score()`                              | [`CWServ.tdxf10_gg_ggzp`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-ggzp)               | 返回综合评分、行业排名、市场排名、资金面 / 基本面 / 消息面 / 主题面评分     | [文档](docs/methods/F10-个股总评.md)      |
| 盈利预测         | `client.f10.profit_forecast()`                          | [`CWServ.tdxf10_gg_ybpj`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-ybpj)               | 返回未来三年 EPS、归母净利润、营业收入预测、历史实际值和预测机构数量         | [文档](docs/methods/F10-盈利预测.md)      |
| 题材概念行情       | `client.f10.theme_market()`                             | [`HQServ.hq_nlp_tcihq`](docs/F10_7615.md#entry-hqserv-hq-nlp-tcihq)                   | 返回相关板块、板块成分股、主力控盘比例、主力资金走势、区间统计等             | [文档](docs/methods/F10-题材概念行情.md)    |
| 估值市场数据       | `client.f10.valuation()`                                | [`HQServ.hq_nlp_gpsj`](docs/F10_7615.md#entry-hqserv-hq-nlp-gpsj)                     | 返回 PE(TTM)、PB(MRQ)、市销率、市现率、估值百分位、流通市值、总市值等   | [文档](docs/methods/F10-估值市场数据.md)    |
| 市场 / 行业排名    | `client.f10.ranking_detail()`                           | [`CWServ.tdxf10_gg_zxts_rqpm`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-zxts-rqpm)     | 返回当前股票排名、排名变化，以及同组股票代码、简称、市场和更新时间            | [文档](docs/methods/F10-市场行业排名.md)    |
| 资本运作治理       | `client.f10.governance()`                               | [`CWServ.tdxf10_gg_zbyz`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-zbyz)               | 返回担保明细、违规处理、处罚公布日、案情进展、处罚决定、详情记录 ID 等        | [文档](docs/methods/F10-资本运作治理.md)    |
| 热点题材         | `client.f10.hot_topics()`                               | [`CWServ.tdxf10_gg_rdtc`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-rdtc)               | 返回题材名称、关联度、入选日期、入选原因、事件名称和事件详情 ID            | [文档](docs/methods/F10-热点题材.md)      |
| 题材内对比        | `client.f10.topic_compare()` / `topic_compare_first()`  | [`CWServ.tdxf10_gg_rdtc_gndb`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-rdtc-gndb)     | 返回题材内股票财务 / 市值 / 涨幅排名，可用于比较同题材个股             | [文档](docs/methods/F10-题材内对比.md)     |
| 公司资讯 / 研报    | `client.f10.company_news()`                             | [`CWServ.tdxf10_gg_gszx`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-gszx)               | 返回研报标题、评级类别、研究员、撰写日期、研报地址，也可查监管措施            | [文档](docs/methods/F10-公司资讯研报.md)    |
| 沪深股通持仓       | `client.f10.northbound_holding()`                       | [`CWServ.tdxf10_gg_zlcc`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-zlcc)               | 返回沪股通 / 深股通持股比例、持股数量、变动股数、收盘价等序列             | [文档](docs/methods/F10-沪深股通持仓.md)    |
| 详情正文         | `client.f10.detail()`                                   | [`CWServ.tdxf10_gg_idreq`](docs/F10_7615.md#entry-cwserv-tdxf10-gg-idreq)             | 按记录 ID 返回正文标题和正文内容；常接热点题材事件、违规处理等详情          | [文档](docs/methods/F10-详情正文.md)      |
| 新闻 / 公告 / 路演 | `client.f10.news()` / `announcements()` / `roadshows()` | [`CWSearch.tzx_rcache`](docs/F10_7615.md#entry-cwsearch-tzx-rcache)                   | 返回新闻、公告、路演列表；含标题、日期、来源、公告类型、PDF 地址等          | [文档](docs/methods/F10-新闻公告路演.md)    |

常规 F10 返回 `F10Response`；`client.f10.limit_board_ladder()` 返回 `LimitBoardLadder`。常用结果分别在 `response.rows` 或模型的 `rows`，完整说明见 [F10_7615.md](docs/F10_7615.md)。

## 连接、并发和缓存

默认 `TdxClient()` 使用真实 `7709` 行情主站。整个过程可以概括为：先从候选主站中测速排名，再选服务器建立多条 TCP 连接，最后由 Rust Runtime 用少量工作线程驱动这些异步连接。

<p align="center">
  <strong>43 台候选主站</strong>
  &nbsp;→&nbsp;
  <strong>TCP 测速并保存排名</strong>
  &nbsp;→&nbsp;
  <strong>默认选最快 2 台</strong>
  &nbsp;→&nbsp;
  <strong>每台 4 TCP</strong>
  &nbsp;→&nbsp;
  <strong>8 Slot</strong>
  &nbsp;→&nbsp;
  <strong>Runtime 按逻辑 CPU 自动取值</strong>
</p>

> **核心关系：** `服务器数 × 每台 TCP 连接数 = Slot 总数`。Slot 数是同时可以在网络上处理的请求数，**不是线程数**。例如 20 台 × 每台 8 条 = 160 Slot；如果当前进程可用 16 个逻辑处理器，默认由 16 个 Runtime 工作线程异步驱动。

### 服务器测速与排名

- 不传 `host` / `hosts` 时，读取包内 `tdx_server.json` 的 43 台候选主站。
- `probe_hosts=True` 默认开启。创建 `TdxClient` 时会阻塞等待普通 43 台和专用 35 台两组候选主站的测速、排序完成；两组地址合并去重后并发探测，再分别保存排名。显式 `host` / `hosts` 只替换普通组，专用组仍供集合竞价和资金流向使用。
- 测速只打开并关闭临时探测连接，不创建业务 Engine、不做协议握手。进入 `with` 或显式 `connect()` 连接普通池；集合竞价与资金流向的独立池仍在首次请求时按需建连、握手，后续复用连接。
- 同一客户端生命周期内复用内存排名，首次请求、另一个专用池启动或专用池重新创建都不重复测速。`probe_hosts=False` 跳过两组自动测速，使用已有排名；内存和自定义非池化 transport 不触发测速。单独使用 `PooledSocketTransport` 仍在首次启动 Engine 时测速。
- 测速检查的是 TCP connect 延迟，用来快速排除连不上或延迟高的主站；它不等于完整业务请求质量测试。
- 排名会原子写入当前用户数据目录的 `tdx_server_ranking.json`。下次启动先使用上次排名，然后重新测速刷新；升级 eltdx 不会覆盖这份用户排名。

也可以手动重测全部 43 台候选主站并持久化结果：

```python
from eltdx.hosts import refresh_server_ranking

ranking = refresh_server_ranking(timeout=1.2)
for item in ranking[:10]:
    print(item)
```

如果不想自动选站，可以直接指定一台主站：

```python
from eltdx import TdxClient

with TdxClient(host="116.205.183.150:7709", timeout=3) as client:
    print(client.helpers.full_quotes("sz000001"))
```

### 服务器、Slot 和线程怎么设

| 场景 | 配置 | 实际含义 |
| --- | --- | --- |
| 默认，适合普通查询 | `TdxClient()` | 最快 2 台 × 每台 4 TCP = 8 Slot；Runtime 线程数自动 |
| 中等并发 | `TdxClient(server_count=4, connections_per_server=8)` | 最快 4 台 × 每台 8 TCP = 32 Slot |
| 高并发 | `TdxClient(server_count=20, connections_per_server=8)` | 最快 20 台 × 每台 8 TCP = 160 Slot |
| 手动限制 CPU 线程 | `TdxClient(server_count=4, connections_per_server=8, runtime_workers=8)` | 仍为 32 Slot，只让 Rust Runtime 用 8 个 worker 驱动 |
| 兼容旧配置 | `TdxClient(pool_size=16)` | `pool_size` 仅表示 TCP/Slot 总数，在选中主站之间尽量平均分配 |

`runtime_workers` 默认为 `min(pool_size, 当前进程可用的逻辑处理器数)`，会识别 Windows、Linux 和 macOS，也会尽量遵守容器或 CPU affinity 对进程的限制。自动值适合大多数情况；手动值必须在 `1..=pool_size` 内。更多 worker 只是提高同时解析/调度的 CPU 能力，不会凭空增加 TCP 连接。

| 参数 | 默认 | 控制什么 |
| --- | --- | --- |
| `server_count` | `2` | 从测速排名中取前几台服务器 |
| `connections_per_server` | `4` | 每台选中服务器建几条 TCP，不是每台几个线程 |
| `pool_size` | 自动为 `server_count × connections_per_server` | TCP/Slot 总数；显式同时传三者时，乘积必须一致 |
| `runtime_workers` | 按 Slot 和逻辑 CPU 自动 | Rust Tokio Runtime 工作线程数 |
| `max_connections_per_host` | 按初始分布自动 | 单台服务器的活动连接硬上限，重连时也遵守 |
| `connect_concurrency` | 按 CPU/Slot 自动，单批最高 `32` | 整个 Engine 每批最多同时建立和握手的 TCP 连接数，**不是 Runtime 线程上限** |
| `connect_concurrency_per_host` | 单批最多 `2` | 每批对同一台服务器最多同时建立 2 条 TCP，也不是线程数 |
| `probe_hosts` | `True` | 创建客户端时是否对两组候选主站测速、持久化并缓存排名 |
| `heartbeat_interval` | `30.0` | 后台心跳秒数；`None` 表示关闭 |
| `max_pending_requests` | `256` | Slot 全忙时允许等待的请求数；满后抛 `PoolBusyError` |
| `push_queue_size` / `push_queue_bytes` | `1024` 帧 / `64 MiB` | 用户可见 push 缓冲的帧数和字节上限 |
| `global_raw_bytes` / `global_decoded_bytes` | 随 Slot 自动增长 | Engine 全局内存预算，分别最高 `256 MiB` / `2 GiB`，防止异常流量无限增长 |

160 Slot 并不会一次性拉起 160 次握手：默认每批全局最多同时建立 32 条 TCP，其中对同一台服务器最多 2 条，完成一批后再继续。这个 32 只控制启动时的建连速度；连好后 160 个 Slot 都能异步工作，Runtime 线程数另由 `runtime_workers` 决定。所有连接共享 Engine 级 raw、decoded 和 push 内存预算；达到上限时会限流、丢弃过旧 push 或关闭受影响的 Slot，不会任由队列无限占用内存。

运行时可通过 diagnostics 查看真实线程数、已连主站和内存占用：

```python
with TdxClient(server_count=4, connections_per_server=8, timeout=3) as client:
    snapshot = client.transport.diagnostics
    print(snapshot.runtime_workers, snapshot.server_count)
    print(client.transport.connected_hosts)
    print(snapshot.raw_bytes, snapshot.decoded_bytes)
```

每个 Engine 使用一个单所有者 Supervisor 统一管理请求生命周期；每个 Slot task 独占自己的 socket、decoder 和 TCP generation。某条连接超时或返回异常时，只废弃当前 Slot 的连接代次，避免迟到响应污染后续请求。

建议始终使用 `with TdxClient(...) as client:`。退出 `with` 时会停止接收新请求、清理 pin/push、关闭 socket 并等待 Runtime 结束。手动创建客户端时，需要在 `finally` 中调用 `client.close()`。

真实 socket 默认每 30 秒自动心跳保活。关闭后台心跳：

```python
client = TdxClient(heartbeat_interval=None)
```

Helpers 会缓存 `stock_profile_table()` 内部使用的财务批次、证券表和已验证的短线统计资源。代码数量、代码表、股本变迁、直接财务查询、实时行情、分时、成交明细和 K 线不缓存。

```python
client.helpers.shortline_indicators("sz000001", refresh_stats=True)
client.clear_cache()
```

## 调试和导出

部分接口支持 `include_raw=True`，用于保留原始 payload 或单条记录 hex，方便排查字段解析问题。

```python
gbbq = client.corporate.capital_changes("sz000001", include_raw=True)
bars = client.bars.get("sz000001", period="day", include_raw=True)
ticks = client.trades.history("sz000001", "2026-05-20", include_raw=True)
```

返回模型可以直接转 JSON：

```python
from eltdx import to_json, to_jsonable

data = to_jsonable(client.helpers.full_quotes("sz000001"))
text = to_json(data, indent=2)
```

真实环境 smoke：

```bash
eltdx-smoke --timeout 6 --no-heartbeat
eltdx-f10-smoke --code 000034 --timeout 8
```

源码仓库可运行更多开发脚本，例如批量导出某段日期的 09:25 竞价成交快照：

```bash
python scripts/smoke/export_auction_925_daily.py --code sz000001 --start 2026-04-01 --end 2026-04-30
```

## 文档

不用按文件名翻目录，按要解决的问题进入即可：

| 想查什么 | 文档入口 |
| --- | --- |
| 搜索全部功能和调用 | [接口目录](https://electkismet.github.io/eltdx/) |
| 常用场景和可复制示例 | [Helpers](docs/helpers/README.md) · [示例](docs/EXAMPLES.md) |
| 方法参数、返回字段和完整 API | [方法手册](docs/METHOD_REFERENCE.md) · [字段手册](docs/FIELD_REFERENCE.md) · [API 参考](docs/API_REFERENCE.md) |
| 底层协议和 F10 Entry | [7709 命令](docs/COMMANDS_7709.md) · [7615 F10](docs/F10_7615.md) |
| 连接、测速、并发和故障排查 | [调试指南](docs/DEBUG_GUIDE.md) · [架构](docs/ARCHITECTURE.md) |
| MCP 安装、工具和资源 | [MCP 文档](docs/MCP.md) |
| 当前版本、变更和旧 API 迁移 | [v3.1.7](docs/releases/v3.1.7.md) · [变更记录](docs/CHANGELOG.md) · [迁移说明](docs/MIGRATION_FROM_OLD.md) |

## 常用问题

- [想拿某个或某些股票的表头信息怎么办？](docs/helpers/股票信息汇总.md)
- [想查询某个股票都有哪些概念板块怎么办？](docs/helpers/个股概念板块.md)
- [想查询某个概念板块都有哪些股票怎么办？](docs/helpers/概念板块成分股.md)
- [想拿集合竞价数据怎么办？](docs/helpers/竞价数据.md)
- [想拿流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比和几天几板怎么办？](docs/helpers/短线指标.md)
- [K 线、自动分页和服务端复权](docs/methods/7709-K线周期线.md)

常用组合调用示例：

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    table = client.helpers.stock_profile_table(["sz000001", "sh600000"])
    shortline = client.helpers.shortline_indicators(["sz000001", "sh600000"])
    topics = client.helpers.stock_topics("000034")
    stocks = client.helpers.topic_stocks("000034", topic_name="存储芯片")
    auction = client.helpers.auction_data("sz000001", "2026-05-20")

print(table.rows[0])
print(shortline.rows[0])
print(topics.topics[:3])
print(stocks.rows[:10])
print(auction.open_price, auction.open_change_pct, auction.open_amount)
```

完整入口见 [docs/helpers/README.md](docs/helpers/README.md)。

## 联系

- QQ 群：[点击链接加入群聊](https://qm.qq.com/q/zAjpZsvfzy)

- 邮箱：[dapaoxixixi@163.com](mailto:dapaoxixixi@163.com)

## 许可证

本项目仅允许个人学习、协议研究和非商业研究使用，禁止一切商业使用和滥用。详细条款见 [LICENSE](LICENSE)。
