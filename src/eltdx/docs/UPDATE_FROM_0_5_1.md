# 从 v0.5.1 到 v1.0.0 的更新说明

!!! warning "历史版本文档"
    本页归档的是 `v1.0.0` 发布时相对 `v0.5.1` 的变化，文中的 `client.get_*()` 示例只适用于当时的 1.x API。当前 `v2.0.5` 已移除这些入口，请按 [迁移到 eltdx 2.0](MIGRATION_FROM_OLD.md) 使用模块化 API。

`eltdx v1.0.0`相比 `v0.5.1`，把行情协议、F10 资料接口、常用场景封装、MCP 工具和中文文档重新整理为一个完整的 Python 客户端。

## 版本定位

`v0.5.1` 主要补充了 MCP 工具和部分行情能力。`v1.0.0` 在此基础上重新设计项目结构，把底层协议解析、业务 API、数据模型、连接管理、F10 HTTP 调用和文档体系拆开维护。

`v1.0.0` 当时推荐从 `TdxClient` 开始使用：

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    quote = client.get_quote("sz000001")
    bars = client.get_kline("day", "sz000001", count=30)
    topics = client.helpers.stock_topics("000034")
```

## 行情能力

`v1.0.0` 新增和整理了 `7709` 二进制行情接口封装，覆盖常见行情、图表、成交、竞价和公司基础数据。

| 能力             | 调用入口                                                | 说明                         |
| -------------- | --------------------------------------------------- | -------------------------- |
| 握手、心跳          | `client.session`                                    | 连接后可自动握手，长连接默认后台心跳保活       |
| 代码数量、代码表       | `client.codes`                                      | 查询沪、深、北市场数量和全量代码表          |
| 批量快照           | `client.quotes.get_snapshots()` / `get_quote()`     | 查询现价、涨跌幅、成交量额和盘口；`get_quote()` 补齐五档盘口 |
| 分类行情           | `client.quotes.list_by_category()`                  | 按市场、板块和排序字段分页查询行情列表        |
| 增量刷新 / 推送队列    | `client.quotes.refresh()` / `poll_push()`           | 支持关注代码刷新和未配对推送帧读取          |
| K 线 / 周期线      | `client.bars.get()` / `get_kline()`                 | 支持分钟、日、周、月、季、年线和服务端复权参数    |
| 当日分时、历史分时、近期分时 | `client.minutes`                                    | 查询主站当日分时和历史分钟走势              |
| 分时副图、小走势图      | `client.minutes.aux()` / `sparkline()`              | 查询分时页副图序列和小型走势序列           |
| 当日成交明细、历史成交明细  | `client.trades`                                     | 查询逐条成交记录，包含时间、价格、成交量、方向等字段 |
| 集合竞价明细         | `client.auctions.series()`                          | 查询主站当前保存的集合竞价阶段明细          |
| 09:25 竞价成交快照   | `client.trades.opening_match_today()` / `opening_match_history()` | 从当日或历史成交明细中提取 09:25 最终竞价成交 |
| 股本变迁 / GBBQ    | `client.corporate.capital_changes()` / `get_gbbq()` | 查询除权除息、股本变化、增发、回购等事件       |
| 财务基础信息         | `client.corporate.finance_batch()`                  | 查询流通股本、总股本、EPS、资产、收入、利润等字段 |
| 特殊品种涨跌停限制      | `client.limits.special()`                           | 查询特殊品种涨跌停限制表               |

底层命令和业务 API 的对应关系见 [COMMANDS_7709.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/COMMANDS_7709.md)。

## F10 / 资料能力

`v1.0.0` 新增 `7615 / TQLEX` HTTP 资料接口封装，可以通过 `client.f10` 或 `F10Client` 查询。

| 能力       | 调用入口                                                       | 说明                         |
| -------- | ---------------------------------------------------------- | -------------------------- |
| 公司概况     | `client.f10.company_profile()`                             | 上市日期、发行方式、发行价、募资额、承销商等     |
| 主营构成     | `client.f10.business_composition()`                        | 主营收入、成本、毛利、收入占比、毛利率等       |
| 财务报表     | `client.f10.finance_report()`                              | 多期资产负债表、利润表、现金流量表等         |
| 财务诊断     | `client.f10.finance_diagnosis()`                           | 营运、盈利、成长、现金流、资产质量和总评分      |
| 个股总评     | `client.f10.stock_score()`                                 | 综合评分、行业排名、市场排名和多维评分        |
| 盈利预测     | `client.f10.profit_forecast()`                             | EPS、归母净利润、营业收入预测和机构数量      |
| 热点题材     | `client.f10.hot_topics()`                                  | 题材名称、关联度、入选日期、入选原因和事件详情 ID |
| 题材内对比    | `client.f10.topic_compare()`                               | 同题材股票的财务、市值、涨幅等排名对比        |
| 题材概念行情   | `client.f10.theme_market()`                                | 相关板块、板块成分股、资金走势和区间统计       |
| 公告、新闻、研报 | `client.f10.news()` / `announcements()` / `company_news()` | 查询公司资讯、公告、研报和路演列表          |
| 详情正文     | `client.f10.detail()`                                      | 按记录 ID 查询正文标题和正文内容         |

完整 Entry 对照见 [F10_7615.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/F10_7615.md)。

## 常用场景 Helper

新版新增 `client.helpers`，把多个底层接口组合成更容易直接使用的场景方法。

| 常用问题                | 调用入口                                   | 文档                            |
| ------------------- | -------------------------------------- | ----------------------------- |
| 想拿某个或某些股票的表头信息怎么办？  | `client.helpers.stock_profile_table()` | [股票信息汇总](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/helpers/股票信息汇总.md)   |
| 想查询某个股票都有哪些概念板块怎么办？ | `client.helpers.stock_topics()`        | [个股概念板块](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/helpers/个股概念板块.md)   |
| 想查询某个概念板块都有哪些股票怎么办？ | `client.helpers.topic_stocks()`        | [概念板块成分股](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/helpers/概念板块成分股.md) |
| 想拿集合竞价数据怎么办？        | `client.helpers.auction_data()`        | [竞价数据](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/helpers/竞价数据.md)       |
| 想拿复权或不复权 K 线怎么办？    | `client.helpers.adjusted_kline()`      | [复权K线](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/helpers/复权K线.md)       |

常用问题入口见 [helpers/README.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/helpers/README.md)。

## 连接、缓存和调试

- 默认 `TdxClient()` 使用真实 `7709` 行情主站。
- 支持 `TdxClient.from_hosts()` 进行主站测速和连接池初始化。
- `SocketTransport` 负责 TCP 长连接，`PooledSocketTransport` 负责连接池。
- 长连接默认每 30 秒自动心跳保活，可用 `heartbeat_interval=None` 关闭。
- 响应按请求编号配对，未配对推送帧进入 push queue。
- 代码数量、全量代码表、股本变迁和财务基础信息会做内存缓存。
- 支持 `include_raw=True` 保留原始 payload 或单条记录 hex，方便排查字段解析。
- 返回模型支持 `to_jsonable()` 和 `to_json()` 转成 JSON 友好结构。

## MCP 工具

新版把 MCP 作为正式能力接入，安装后可以启动：

```bash
eltdx-mcp
```

当前 MCP 工具覆盖行情快照、K 线、个股题材、题材成分股、F10 概况、热点题材和 09:25 竞价成交快照。详细说明见 [MCP.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/MCP.md)。

## 文档体系

`v1.0.0` 补充了面向使用者的中文文档：

- [README](https://github.com/electkismet/eltdx/blob/v1.0.0/README.md)：项目能力、安装方式、快速开始和文档入口。
- [METHOD_REFERENCE.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/METHOD_REFERENCE.md)：每个调用方法的参数、底层接口和解析字段。
- [methods/README.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/methods/README.md)：每个调用方法的独立说明页。
- [FIELD_REFERENCE.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/FIELD_REFERENCE.md)：返回模型字段中文含义。
- [COMMANDS_7709.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/COMMANDS_7709.md)：`7709` 命令和业务 API 对照。
- [F10_7615.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/F10_7615.md)：`7615 / TQLEX` Entry 和资料接口说明。
- [API_REFERENCE.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/API_REFERENCE.md)：`TdxClient`、`F10Client` 和业务 API 说明。
- [EXAMPLES.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/EXAMPLES.md)：复制即用的调用示例。
- [DEBUG_GUIDE.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/DEBUG_GUIDE.md)：主站、连接和协议排查。
- [MCP.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/MCP.md)：MCP 工具服务说明。

## 与旧版的主要差异

| 方向     | v0.5.1                     | v1.0.0                                                                   |
| ------ | -------------------------- | ------------------------------------------------------------------------ |
| 项目结构   | 功能较集中，历史实现和实验能力混在一起        | 拆分为 `client / api / protocol / transport / models / f10 / helpers / mcp` |
| 行情接口   | 覆盖部分常用行情和 MCP 工具           | 系统整理 19 个 `7709` 命令，并提供业务 API 和字段文档                                      |
| F10 资料 | 覆盖较少                       | 新增 `7615 / TQLEX` F10 资料接口                                               |
| 调用方式   | 部分旧函数和脚本入口                 | 统一通过 `TdxClient`、`F10Client` 和 `client.helpers` 调用                       |
| 文档     | 以 README、CHANGELOG 和少量说明为主 | 中文 README、方法文档、字段手册、协议对照、F10 文档、MCP 文档                                   |
| MCP    | 已有工具补充                     | 作为正式入口 `eltdx-mcp` 接入新版 API                                              |
| 缓存和连接  | 基础连接能力                     | 主站测速、连接池、自动心跳、响应配对、推送队列和低频缓存                                             |

## v1.0.0 当时的迁移建议

以下示例保留当时发布内容，不适用于当前 `v2.0.5`。当前代码请查看 [迁移到 eltdx 2.0](MIGRATION_FROM_OLD.md)。

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    quote = client.get_quote(["sz000001", "sh600000"])
    kline = client.get_kline("day", "sz000001", count=100)
    f10 = client.f10.company_profile("000034")
```

旧项目迁移时，建议按业务类型替换：

| 原使用目的     | 新版入口                                                                          |
| --------- | ----------------------------------------------------------------------------- |
| 查询行情快照    | `client.get_quote()` 或 `client.quotes.get_snapshots()`                        |
| 查询 K 线    | `client.get_kline()` 或 `client.bars.get()`                                    |
| 查询分时      | `client.get_minute()` / `client.minutes.today()` / `client.minutes.history()` |
| 查询成交明细    | `client.get_trades()` / `client.trades.today()` / `client.trades.history()`   |
| 查询股本变迁    | `client.get_gbbq()` / `client.corporate.capital_changes()`                    |
| 查询除权除息    | `client.get_xdxr()`                                                           |
| 查询题材概念    | `client.helpers.stock_topics()` / `client.f10.hot_topics()`                   |
| 查询 F10 资料 | `client.f10` 或 `F10Client`                                                    |
| 启动 MCP    | `eltdx-mcp`                                                                   |

旧字段和当前字段的关系见 [FIELD_MIGRATION.md](https://github.com/electkismet/eltdx/blob/v1.0.0/docs/FIELD_MIGRATION.md)。

## 安装和验证

```bash
pip install eltdx
```

源码开发：

```bash
pip install -e ".[dev]"
python -m pytest
```

真实环境 smoke：

```bash
eltdx-smoke --timeout 6 --no-heartbeat
eltdx-f10-smoke --code 000034 --timeout 8
```

## 使用限制

本项目仅允许个人学习、协议研究和非商业研究使用，禁止一切商业使用和滥用。使用者访问第三方服务器或服务时，需要自行遵守相关法律法规及服务协议。
