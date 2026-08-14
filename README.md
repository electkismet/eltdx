<h1 align="center">eltdx</h1>

<p align="center">
  <strong>极简的通达信在线行情协议 Python 客户端</strong>
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
  <img alt="eltdx 接口目录预览" src=".github/assets/eltdx-readme-banner.png">
</a>

<p align="center">
  <a href="https://api.astlane.com/">
    <img alt="Astlane token 赞助方" src="docs/assets/astlane-sponsor.svg" width="860">
  </a>
</p>

> 如果需要多数据源可以关注新项目 [AxData](https://github.com/electkismet/AxData)：AxData 基于 eltdx 迭代开发，除通达信体系外，AxData 还通过插件机制整理接入交易所、巨潮、腾讯财经、新浪财经、东方财富、财联社、开盘红等公开源接口，并扩展了自由流通市值、开盘换手、开盘量比、开盘抢筹、竞价昨比、连板天梯、题材强度等更适合本地量化研究和短线数据分析的指标能力，但如果是用单一源的话，仍推荐eltdx，受制于架构axdata性能不如裸协议路径的eltdx。

通达信在线行情协议 Python 库。可以拿 A 股的行情、分时、成交明细、K 线、竞价、公司信息、题材信息等信息，支持 MCP 工具。

> `v2.0.0` 移除 `TdxClient` 的旧版扁平旧版入口，统一使用模块化 API 和 Helpers。升级前请阅读 [v2.0.0 迁移说明](docs/releases/v2.0.0.md)。

1. 本项目仅以个人学习、协议研究和非商业研究为目的进行开发。
2. 本项目基于互联网公开信息搜集开发。
3. 项目本身、衍生产品及通过本项目获取的数据禁止用于任何商业行为、付费服务、生产服务、转售或其他营利用途，产生的任何数据、损失或法律责任由使用者自负。
4. 对第三方服务器或服务的访问，用户需自行遵守相关法律法规及服务协议。
5. 请勿将本项目用于侵犯他人权益、违反监管规定或滥用第三方服务的行为。

感谢 [injoyai/tdx](https://github.com/injoyai/tdx) 及 [rainx/pytdx](https://github.com/rainx/pytdx) 的启发。

## eltdx 功能简介

| 类型     | 能力                                          | 入口                                 |
| ------ | ------------------------------------------- | ---------------------------------- |
| 行情基础   | 代码表、代码数量、批量行情查询/推送、分类行情                     | `client.codes`、`client.quotes`     |
| 图表数据   | K 线、当日分时、历史分时、近期分时、分时副图、小走势图                | `client.bars`、`client.minutes`     |
| 成交数据   | 当日成交明细、历史成交明细、当日集合竞价过程快照、09:25 正式撮合           | `client.trades`、`client.auctions`  |
| 公司基础   | 股本变迁、除权除息、财务基础信息、特殊品种涨跌停限制                  | `client.corporate`、`client.limits` |
| F10 资料 | 公司概况、热点题材、公告、新闻、研报、财务报表、估值、主营构成             | `client.f10` 或 `F10Client`         |
| 常用场景   | 股票信息汇总、短线指标（流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比、几天几板等）、个股概念板块、概念板块成分股、竞价数据、复权/不复权 K 线 | `client.helpers`                   |
| 工具能力   | 连接池、主站测速、自动心跳、低频数据缓存、JSON 序列化、交易日工具、MCP 工具服务 | `TdxClient`、`WorkdayService`、`eltdx-mcp` |

完整接口目录见 [GitHub Pages](https://electkismet.github.io/eltdx/)。调用方法和返回字段看 [METHOD_REFERENCE.md](docs/METHOD_REFERENCE.md)，常用问题入口看 [docs/helpers/README.md](docs/helpers/README.md)，完整 API 看 [API_REFERENCE.md](docs/API_REFERENCE.md)，字段总表看 [FIELD_REFERENCE.md](docs/FIELD_REFERENCE.md)，F10 资料看 [F10_7615.md](docs/F10_7615.md)，MCP 工具看 [MCP.md](docs/MCP.md)。

## 安装

```bash
pip install eltdx
```

如果需要启动 MCP stdio 工具服务，安装可选依赖：

```bash
pip install "eltdx[mcp]"
```

源码目录安装：

```bash
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -U pip
pip install -e ".[dev,mcp]"
```

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
| K 线 / 周期线    | `client.bars.get(code, ...)`                        | [`0x052d`](docs/COMMANDS_7709.md#cmd-0x052d)                                                | 单页返回 OHLC、成交量额、前收等 K 线；支持分钟、日、周、月、季、年线和服务端复权参数 | [文档](docs/methods/7709-K线周期线.md)      |
| 全量 K 线分页     | `client.bars.all()`                    | [`0x052d`](docs/COMMANDS_7709.md#cmd-0x052d)                                                | 自动按页拉取 K 线并合并；适合补历史日线或分钟线数据                                    | [文档](docs/methods/7709-全量K线分页.md)     |
| 当日分时         | `client.minutes.today()`                  | [`0x0537`](docs/COMMANDS_7709.md#cmd-0x0537)                                                | 返回主站当前保存的每分钟价格、成交量、均价等分时序列                                       | [文档](docs/methods/7709-当日分时.md)       |
| 指定日期历史分时     | `client.minutes.history()`        | [`0x0fb4`](docs/COMMANDS_7709.md#cmd-0x0fb4)                                                | 按日期返回某天的分时价格和分钟成交量，适合补单日历史分时                                   | [文档](docs/methods/7709-指定日期历史分时.md)   |
| 近期历史分时       | `client.minutes.recent()`                                  | [`0x0feb`](docs/COMMANDS_7709.md#cmd-0x0feb)                                                | 返回服务端近期窗口内的历史分时；适合查较近交易日的分钟走势                                  | [文档](docs/methods/7709-近期历史分时.md)     |
| 分时副图         | `client.minutes.aux()`                                     | [`0x051b`](docs/COMMANDS_7709.md#cmd-0x051b)                                                | 返回分时页下方副图数据，例如买卖力道、成交对比等序列                                     | [文档](docs/methods/7709-分时副图.md)       |
| 小走势图         | `client.minutes.sparkline()`                               | [`0x0fd1`](docs/COMMANDS_7709.md#cmd-0x0fd1)                                                | 返回单标的小型价格走势序列，适合列表页或概览页的小图                                     | [文档](docs/methods/7709-小走势图.md)       |
| 当日成交明细       | `client.trades.today(code, ...)` / `client.trades.all_today(code, ...)`                   | [`0x0fc5`](docs/COMMANDS_7709.md#cmd-0x0fc5)                                                | `today()` 返回一页，`all_today()` 自动翻页返回完整明细；混合记录中可能包含普通成交、竞价快照和 09:25 正式撮合                       | [文档](docs/methods/7709-当日成交明细.md)     |
| 当日成交明细竞价记录 | `client.trades.auction_today(code, ...)` | [`0x0fc5`](docs/COMMANDS_7709.md#cmd-0x0fc5) | 从当日成交明细筛选 `status=8` 竞价记录；时间字段只有分钟 | [文档](docs/methods/7709-当日成交明细竞价记录.md) |
| 当日 09:25 正式撮合 | `client.trades.opening_match_today(code, ...)` | [`0x0fc5`](docs/COMMANDS_7709.md#cmd-0x0fc5) | 从当日成交明细筛选 09:25 正式撮合；没有记录时返回 `None` | [文档](docs/methods/7709-当日0925正式撮合.md) |
| 历史成交明细       | `client.trades.history(code, date, ...)` / `client.trades.all_history(code, date, ...)`      | [`0x0fc6`](docs/COMMANDS_7709.md#cmd-0x0fc6)                                                | `history()` 返回一页，`all_history()` 自动翻页返回指定交易日混合明细                      | [文档](docs/methods/7709-历史成交明细.md)     |
| 历史成交明细竞价记录 | `client.trades.auction_history(code, date, ...)` | [`0x0fc6`](docs/COMMANDS_7709.md#cmd-0x0fc6) | 从历史成交明细筛选 `status=8` 竞价记录；时间字段只有分钟 | [文档](docs/methods/7709-历史成交明细竞价记录.md) |
| 历史 09:25 正式撮合 | `client.trades.opening_match_history(code, date, ...)` | [`0x0fc6`](docs/COMMANDS_7709.md#cmd-0x0fc6) | 从历史成交明细筛选指定日期 09:25 正式撮合；没有记录时返回 `None` | [文档](docs/methods/7709-历史0925正式撮合.md) |
| 当日集合竞价过程快照 | `client.auctions.series()`          | [`0x056a`](docs/COMMANDS_7709.md#cmd-0x056a)                                                | 返回竞价过程快照，正文说明其秒字段和通常几秒一条的特点；即使出现 `09:25:00` 也不是正式成交                       | [文档](docs/methods/7709-集合竞价明细.md)     |
| 股本变迁 / GBBQ  | `client.corporate.capital_changes()` / `client.helpers.capital_changes()`        | [`0x000f`](docs/COMMANDS_7709.md#cmd-0x000f)                                                | 返回除权除息、股本变化、增发、回购等股本事件记录                                       | [文档](docs/methods/7709-股本变迁GBBQ.md)   |
| 除权除息整理       | `client.helpers.xdxr()`                                        | [`0x000f`](docs/COMMANDS_7709.md#cmd-0x000f)                                                | 从股本变迁里筛出除权除息事件，整理分红、送转、配股等字段                                   | [文档](docs/methods/7709-除权除息整理.md)     |
| 指定日期股本       | `client.helpers.equity()`                                      | [`0x000f`](docs/COMMANDS_7709.md#cmd-0x000f)                                                | 从股本变化记录中取某日期之前最近一次流通股本和总股本                                     | [文档](docs/methods/7709-指定日期股本.md)     |
| 换手率          | `client.helpers.turnover()`                                    | [`0x000f`](docs/COMMANDS_7709.md#cmd-0x000f)                                                | 用成交量和流通股本本地计算换手率；服务器不直接返回这个结果                                  | [文档](docs/methods/7709-换手率.md)        |
| 本地复权因子       | `client.helpers.factors()`                                     | [`0x052d`](docs/COMMANDS_7709.md#cmd-0x052d) + [`0x000f`](docs/COMMANDS_7709.md#cmd-0x000f) | 用不复权日 K 和除权除息记录计算本地前复权 / 后复权因子                                 | [文档](docs/methods/7709-本地复权因子.md)     |
| 财务基础信息       | `client.corporate.finance_batch(codes)` | [`0x0010`](docs/COMMANDS_7709.md#cmd-0x0010)                                                | 批量返回流通股本、总股本、EPS、资产、负债、收入、利润等基础财务字段                            | [文档](docs/methods/7709-财务基础信息.md)     |
| 特殊品种涨跌停限制    | `client.limits.special()`                                  | [`0x0452`](docs/COMMANDS_7709.md#cmd-0x0452)                                                | 返回特殊品种涨跌停限制表；需要按表扫描后本地索引到具体代码                                  | [文档](docs/methods/7709-特殊品种涨跌停限制.md)  |
| 服务器文件分块读取      | `client.resources.read()` | [`0x06b9`](docs/COMMANDS_7709.md#cmd-0x06b9) | 读取一个服务器文件块，返回原始 bytes 和长度头 | [文档](docs/methods/7709-服务器文件读取.md)    |
| 服务器文件下载      | `client.resources.download_file()` | [`0x06b9`](docs/COMMANDS_7709.md#cmd-0x06b9) | 循环读取并合并完整服务器文件 | [文档](docs/methods/7709-服务器文件下载.md)    |
| 服务器统计文件解析      | `client.resources.read_stats()` | [`0x06b9`](docs/COMMANDS_7709.md#cmd-0x06b9) | 下载并解析 `zhb.zip` 中的统计文件 | [文档](docs/methods/7709-服务器统计文件解析.md)    |
| 短线指标（Helper） | `client.helpers.shortline_indicators()`                    | `0x06b9 + 0x054c + 0x0547 + 0x044d + 0x052d`                                             | 按交易日对齐统计资源和实时行情，返回流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比、几天几板等 21 个字段 | [文档](docs/helpers/短线指标.md)              |

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

F10 返回统一是 `F10Response`。常用结果在 `response.rows`，多表结果在 `response.tables`。完整说明见 [F10_7615.md](docs/F10_7615.md)。

## 连接、并发和缓存

默认 `TdxClient()` 使用真实 `7709` 行情主站；不传 `host` / `hosts` 时，会读取包内 `tdx_server.json`。

```python
from eltdx import TdxClient

with TdxClient(host="116.205.183.150:7709", timeout=3) as client:
    print(client.helpers.full_quotes("sz000001"))

with TdxClient.from_hosts(pool_size=4, probe_hosts=True, timeout=3) as client:
    print(client.codes.count("sz"))
```

`pool_size` 默认是 `1`。`pool_size=N` 表示创建 N 个长期 `ConnectionActor` 和 N 条 TCP 连接，最多允许 N 个行情网络请求同时在途；它不表示连接 N 个行情服务器。传入多个主站时，每个连接槽位会按不同顺序尝试这些候选主站，连接失败或断开后自动尝试后续主站。

普通顺序调用保持默认 `1` 即可；需要同时发起多个行情请求时，可以根据实际并发量设置 `pool_size=4`、`pool_size=8` 等正整数。`0`、负数、float、字符串、bool 和 `None` 会直接抛出 `ValueError`，不会被自动修改。

每个连接槽位的 Actor 在线程生命周期内持续复用。网络重连只替换它持有的 TCP socket，不会为每次重连再创建一条 reader 线程。建议始终使用 `with TdxClient(...) as client:`；退出 `with` 时会关闭 Actor、socket、selector 和 wakeup。手动创建客户端时，需要在 `finally` 中调用 `client.close()`。

真实 socket 连接默认每 30 秒自动心跳保活。关闭后台心跳：

```python
client = TdxClient(heartbeat_interval=None)
```

Helpers 会缓存股本变迁、`stock_profile_table()` 内部使用的财务批次和已验证的短线统计资源。代码数量、代码表、直接财务查询、实时行情、分时、成交明细和 K 线不缓存。

```python
client.helpers.capital_changes("sz000001", refresh=True)
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

使用文档现在按层级拆开看：

| 层级           | 看哪里                                                  | 适合解决什么问题                   |
| ------------ | ---------------------------------------------------- | -------------------------- |
| 快速总览         | 本 README                                             | 这个库能查什么、用哪个方法、底层接口是什么      |
| 常用问题         | [docs/helpers/README.md](docs/helpers/README.md)     | 按问题进入对应调用说明             |
| 当前版本         | [docs/releases/v2.0.3.md](docs/releases/v2.0.3.md)   | `v2.0.3` 修复竞价开关、成交分页和发布检查 |
| 变更记录         | [docs/CHANGELOG.md](docs/CHANGELOG.md)               | 当前版本和未发布改动               |
| 迁移到 2.0       | [docs/MIGRATION_FROM_OLD.md](docs/MIGRATION_FROM_OLD.md) | 把 1.x 的旧 `get_*` 调用迁移到当前模块化 API |
| 方法字段手册       | [docs/METHOD_REFERENCE.md](docs/METHOD_REFERENCE.md) | 每个调用方法怎么传参、返回哪些解析字段        |
| 具体调用         | [docs/API_REFERENCE.md](docs/API_REFERENCE.md)       | 每组 API 怎么传参数、返回什么、有哪些注意点   |
| 7709 接口对照    | [docs/COMMANDS_7709.md](docs/COMMANDS_7709.md)       | 21 个二进制命令分别对应哪个业务 API      |
| F10 Entry 对照 | [docs/F10_7615.md](docs/F10_7615.md)                 | 7615/TQLEX 每个资料 Entry 怎么调用 |
| 字段说明         | [docs/FIELD_REFERENCE.md](docs/FIELD_REFERENCE.md)   | 返回模型里的字段中文含义               |

| 文档                                                       | 内容                     |
| -------------------------------------------------------- | ---------------------- |
| [docs/README.md](docs/README.md)                         | 文档入口                   |
| [docs/PRODUCT.md](docs/PRODUCT.md)                       | 产品定位和能力总览              |
| [docs/releases/v2.0.3.md](docs/releases/v2.0.3.md)       | `v2.0.3` 正式发布说明          |
| [docs/releases/v2.0.2.md](docs/releases/v2.0.2.md)       | `v2.0.2` 正式发布说明          |
| [docs/releases/v2.0.1.md](docs/releases/v2.0.1.md)       | `v2.0.1` 正式发布说明          |
| [docs/releases/v2.0.0.md](docs/releases/v2.0.0.md)       | `v2.0.0` 正式发布说明          |
| [docs/releases/v1.3.1.md](docs/releases/v1.3.1.md)       | `v1.3.1` 正式发布说明          |
| [docs/releases/v1.3.0.md](docs/releases/v1.3.0.md)       | `v1.3.0` 正式发布说明          |
| [docs/releases/v1.2.0.md](docs/releases/v1.2.0.md)       | `v1.2.0` 正式发布说明          |
| [docs/releases/v1.1.0.md](docs/releases/v1.1.0.md)       | `v1.1.0` 正式发布说明          |
| [docs/CHANGELOG.md](docs/CHANGELOG.md)                   | 版本变更记录                 |
| [docs/UPDATE_FROM_0_5_1.md](docs/UPDATE_FROM_0_5_1.md)   | 历史归档：从 `v0.5.1` 到 `v1.0.0` 的更新说明 |
| [docs/helpers/README.md](docs/helpers/README.md)         | 常用问题入口                |
| [docs/METHOD_REFERENCE.md](docs/METHOD_REFERENCE.md)     | 调用方法、参数和解析字段           |
| [docs/methods/README.md](docs/methods/README.md)         | 每个调用方法的单页说明            |
| [docs/API_REFERENCE.md](docs/API_REFERENCE.md)           | API 调用说明               |
| [docs/EXAMPLES.md](docs/EXAMPLES.md)                     | 常见复制即用示例               |
| [docs/FIELD_REFERENCE.md](docs/FIELD_REFERENCE.md)       | 返回字段中文含义               |
| [docs/F10_7615.md](docs/F10_7615.md)                     | 7615 F10 / TQLEX 调用说明  |
| [docs/MCP.md](docs/MCP.md)                               | MCP 工具说明               |
| [docs/COMMANDS_7709.md](docs/COMMANDS_7709.md)           | 21 个 `7709` 命令和 API 映射 |
| [docs/DEBUG_GUIDE.md](docs/DEBUG_GUIDE.md)               | 连接、主站和协议排查             |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)             | 项目分层和实现结构              |
| [docs/FIELD_MIGRATION.md](docs/FIELD_MIGRATION.md)       | 历史字段迁移到当前 2.0.3 模型       |
| [docs/MIGRATION_FROM_OLD.md](docs/MIGRATION_FROM_OLD.md) | 1.x API 迁移到 2.0 模块化 API  |
| [docs/ROADMAP.md](docs/ROADMAP.md)                       | 历史归档：1.0 实现记录          |
| [scripts/README.md](scripts/README.md)                   | smoke / live 脚本说明      |

## 常用问题

- [想拿某个或某些股票的表头信息怎么办？](docs/helpers/股票信息汇总.md)
- [想查询某个股票都有哪些概念板块怎么办？](docs/helpers/个股概念板块.md)
- [想查询某个概念板块都有哪些股票怎么办？](docs/helpers/概念板块成分股.md)
- [想拿集合竞价数据怎么办？](docs/helpers/竞价数据.md)
- [想拿流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比和几天几板怎么办？](docs/helpers/短线指标.md)
- [想拿复权或不复权 K 线怎么办？](docs/helpers/复权K线.md)

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
