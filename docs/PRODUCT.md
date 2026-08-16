# eltdx 1.0 产品说明

## 一句话定位

`eltdx` 是一个面向通达信数据源的 Python 客户端：`7709` 负责行情数据，`7615` 负责 F10 / 资料数据。

## 目标用户

| 用户 | 使用场景 |
| --- | --- |
| Python 量化研究者 | 拉取 A 股行情数据，做研究、回测和数据校验 |
| 数据工程开发者 | 把通达信行情源接入自己的本地数据管道 |
| 协议研究者 | 在清晰 API 之上继续验证字段、样本和协议细节 |

## 核心价值

| 问题 | eltdx 的处理方式 |
| --- | --- |
| 底层协议是二进制 TCP 帧，直接用起来不直观 | 对外提供 `client.quotes`、`client.bars`、`client.minutes` 等业务 API |
| 命令号很多，记忆成本高 | 命令号集中放在 `protocol.commands`，业务代码不直接依赖 `0x054c` |
| 模块职责需要清楚 | 拆分客户端、业务 API、transport、protocol、models |
| 协议字段需要可追溯 | 字段解释以仓库内协议文档和 API 参考为准 |

## 1.0 能力范围

| 能力 | API 入口 | 主要命令 |
| --- | --- | --- |
| 连接握手、心跳 | `client.session` | `0x000d`, `0x0004` |
| 代码数量、代码表 | `client.codes` | `0x044e`, `0x044d` |
| 批量快照、旧版批量行情、分类行情、行情刷新协议 | `client.quotes` | `0x054c`, `0x053e`, `0x054b`, `0x0547` |
| 服务器文件读取、整文件下载与 `zhb.zip` 解析 | `client.resources` | `0x06b9` |
| K 线 / 周期线 | `client.bars` | `0x052d` |
| 当日分时、历史分时、近期分时 | `client.minutes` | `0x0537`, `0x0fb4`, `0x0feb` |
| 分时副图、小走势图 | `client.minutes` | `0x051b`, `0x0fd1` |
| 当日成交明细、历史成交明细 | `client.trades` | `0x0fc5`, `0x0fc6` |
| 集合竞价过程快照 | `client.auctions` | `0x056a`，支持当日和历史日期 |
| 股本变迁、财务批量 | `client.corporate` | `0x000f`, `0x0010` |
| 特殊品种涨跌停限制 | `client.limits` | `0x0452` |
| F10、题材、公告、财务报表、估值 | `client.f10` | `7615/TQLEX` |
| 常用场景组合调用（含 21 个短线指标） | `client.helpers` | 组合 7709 / F10 |
| MCP 工具服务 | `eltdx-mcp` | 组合 7709 / F10 |

## 对外 API 风格

使用者按业务调用。真实 7709 主站连接使用：

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    count = client.codes.count("sz")
    page = client.codes.list("sz", start=0, limit=3)
```

需要指定主站、连接池或启动前测速时：

```python
with TdxClient(host="116.205.183.150:7709", timeout=3) as client:
    quote = client.helpers.full_quotes("sz000001")

with TdxClient.from_hosts(pool_size=2, probe_hosts=True, timeout=3) as client:
    count = client.codes.count("sz")
```

F10 资料数据使用独立的 `client.f10`：

```python
client = TdxClient(timeout=3)
profile = client.f10.company_profile("000034")
topics = client.f10.hot_topics("000034")
```

常用场景使用 `client.helpers`：

```python
with TdxClient(timeout=3) as client:
    table = client.helpers.stock_profile_table(["sz000001", "sh600000"])
    shortline = client.helpers.shortline_indicators(["sz000001", "sh600000"])
    topics = client.helpers.stock_topics("000034")
    auction = client.helpers.auction_data("sz000001", "2026-05-20")
```

`shortline` 包含流通市值Z、开盘换手Z、竞价昨比、开盘昨封比、昨封比、封流比、几天几板等 21 个字段，完整口径见[短线指标](helpers/短线指标.md)。

开发和测试时可以使用内存 transport：

```python
from eltdx import TdxClient

with TdxClient.in_memory() as client:
    request = client.codes.count("sz")
    assert request["command"] == "0x044e"
```

底层命令号仍然保留，可以用于调试和追溯：

```python
from eltdx.protocol import COMMANDS

print(COMMANDS["snapshots"].hex)  # 0x054c
```

支持 MCP 的客户端可以启动工具服务：

```bash
eltdx-mcp
```

## 1.0 能力状态

当前项目已经完成 `TdxClient`、业务 API 分层、21 个 `7709` 命令注册表、中文文档、同步 `SocketTransport`、连接池包装、主站 TCP 测速和 MCP 工具服务。

已经真实接入并验证的命令：

| 范围 | 状态 |
| --- | --- |
| 握手、心跳、代码表、快照、K 线、分时、成交明细、集合竞价、股本变迁、财务、特殊涨跌停限制 | 已完成真实主站小样本验证 |
| `0x0547` 行情增量刷新 | 单次主动刷新已验证，最多 100 个代码；推送队列已接入 |
| `7615` F10 / HTTP 网关 | 公司概况、热点题材、公告、题材行情、估值已完成真实小样本验证 |
| MCP 工具服务 | 已迁移到 MCP Python SDK 2，提供 17 个行情、分时、成交、竞价、短线指标、题材和 F10 tools，以及 8 个内置文档 resources |

默认 `TdxClient()` 使用真实行情连接；内存 transport 只用于开发期验证 API 形状和单元测试。
