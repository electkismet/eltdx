# 7709 命令映射

| 命令                              | 文档                        | 业务能力        | API                                                     | 接入情况         |
| ------------------------------- | ------------------------- | ----------- | ------------------------------------------------------- | ------------ |
| <a id="cmd-0x0004"></a>`0x0004` | `0x0004-心跳保活接口.md`        | 心跳保活        | `client.session.heartbeat()`                            | 核心           |
| <a id="cmd-0x000d"></a>`0x000d` | `0x000d-连接握手接口.md`        | 连接握手        | `client.session.handshake()`                            | 核心           |
| <a id="cmd-0x000f"></a>`0x000f` | `0x000f-股本变迁查询接口.md`      | 股本变迁 / 除权基础 | `client.corporate.capital_changes()`                    | 已接入          |
| <a id="cmd-0x0010"></a>`0x0010` | `0x0010-财务信息批量查询&下发接口.md` | 财务信息批量查询    | `client.corporate.finance_batch()`                      | 已接入          |
| <a id="cmd-0x044d"></a>`0x044d` | `0x044d-代码表分页接口.md`       | 代码表分页       | `client.codes.list()`                                   | 核心           |
| <a id="cmd-0x044e"></a>`0x044e` | `0x044e-代码数量接口.md`        | 代码数量        | `client.codes.count()`                                  | 核心           |
| <a id="cmd-0x0452"></a>`0x0452` | `0x0452-特殊品种涨跌停限制表接口.md`  | 特殊品种涨跌停限制   | `client.limits.special()`                               | 已接入          |
| <a id="cmd-0x051b"></a>`0x051b` | `0x051b-个股分时副图数据接口.md`    | 分时副图        | `client.minutes.aux()`                                  | 已接入          |
| <a id="cmd-0x052d"></a>`0x052d` | `0x052d-K线周期数据接口.md`      | K线 / 周期线    | `client.bars.get()`                                     | 核心           |
| <a id="cmd-0x0537"></a>`0x0537` | `0x0537-个股当前日分时图接口.md`    | 当日分时        | `client.minutes.today()`                                | 核心           |
| <a id="cmd-0x053e"></a>`0x053e` | `0x053e-旧版批量行情接口.md`       | 旧版批量行情      | `client.quotes.legacy()` / `client.get_legacy_quotes()` | 已接入          |
| <a id="cmd-0x0547"></a>`0x0547` | `0x0547-行情增量刷新推送接口.md`    | 行情增量刷新 / 推送 | `client.quotes.refresh()` / `client.quotes.poll_push()` | 单次刷新和推送队列已接入 |
| <a id="cmd-0x054b"></a>`0x054b` | `0x054b-分类行情列表分页接口.md`    | 分类行情列表      | `client.quotes.list_by_category()`                      | 核心           |
| <a id="cmd-0x054c"></a>`0x054c` | `0x054c-显式代码批量行情快照接口.md`  | 批量行情快照      | `client.quotes.get_snapshots()`                         | 核心           |
| <a id="cmd-0x056a"></a>`0x056a` | `0x056a-集合竞价明细接口.md`      | 集合竞价明细      | `client.auctions.series()`                              | 已接入          |
| <a id="cmd-0x06b9"></a>`0x06b9` | `0x06b9-服务器文件读取接口.md`     | 服务器文件读取     | `client.resources.read()` / `download_file()` / `read_stats()` | 已接入          |
| <a id="cmd-0x0fb4"></a>`0x0fb4` | `0x0fb4-历史分时数据接口.md`      | 指定日期历史分时    | `client.minutes.history()`                              | 核心           |
| <a id="cmd-0x0fc5"></a>`0x0fc5` | `0x0fc5-当日成交明细分页接口.md`    | 当日成交明细      | `client.trades.today()`                                 | 核心           |
| <a id="cmd-0x0fc6"></a>`0x0fc6` | `0x0fc6-历史成交明细增强分页接口.md`  | 历史成交明细增强    | `client.trades.history()`                               | 已接入          |
| <a id="cmd-0x0fd1"></a>`0x0fd1` | `0x0fd1-单标的价格小走势图接口.md`   | 小走势图        | `client.minutes.sparkline()`                            | 已接入          |
| <a id="cmd-0x0feb"></a>`0x0feb` | `0x0feb-近期历史分时图接口.md`     | 近期历史分时      | `client.minutes.recent()`                               | 已接入          |

说明：`0x0547` 已完成请求构造、响应解析、单次刷新和未配对推送队列。实盘主站对单次超过 100 个代码的请求只返回前 100 条，因此构包层将单次上限固定为 100，避免静默截断。`0x06b9` 的协议层返回单个 `FileContentChunk`；整文件下载和 `zhb.zip` 内容解析位于上层封装。

## 三个行情命令的边界

`0x054c`、`0x0547` 和 `0x053e` 都能返回当前行情字段，但请求语义、二进制布局和使用方式不同，不能互换解析器。

| 命令 | 核心定位 | 请求方式 | 盘口 | 特有能力 | 推荐场景 |
| --- | --- | --- | --- | --- | --- |
| [`0x054c`](methods/7709-批量快照.md) | 当前基础快照 | 显式代码列表，无游标 | 实盘稳定确认买一 / 卖一 | 轻量取得现价、昨收、成交量额等基线字段 | 初始化一批代码的当前行情 |
| [`0x0547`](methods/7709-增量刷新推送队列.md) | 行情增量刷新 | 最多 100 个代码 + 每只代码的游标 | 买一到买五 / 卖一到卖五 | 首次游标 `0` 可取完整五档，之后持续刷新；配合推送队列 | 补齐五档或连续更新行情 |
| [`0x053e`](methods/7709-旧版批量行情.md) | 旧版完整快照 | 显式代码列表，无游标 | 买一到买五 / 卖一到卖五 | 交易状态、尾部指标和旧版协议原始字段 | 兼容旧协议或研究旧版状态字段 |

常规调用使用 `client.get_quote(codes)`：先用 `0x054c` 建立基础快照，再用首次 `0x0547` 刷新补齐五档。`0x053e` 是独立的旧版查询路径，不参与这个组合。

## 命名原则

对外 API 不使用 `0x054c` 这种命令号命名，命令号只保留在 `eltdx.protocol.commands` 中。

这样做的好处：

1. 使用者按业务理解接口。
2. 底层命令调整时，对外 API 可以保持稳定。
3. 文档仍然能从 API 反查到底层协议证据。
