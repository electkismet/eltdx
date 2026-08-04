# MCP 工具

`eltdx` 使用 MCP Python SDK 2 提供 stdio 服务。服务覆盖 7709 行情、K 线、分时、成交、竞价、短线指标，以及 7615 F10 和题材数据。

## 安装和启动

```bash
pip install "eltdx[mcp]"
eltdx-mcp
```

源码目录运行：

```bash
pip install -e ".[mcp]"
eltdx-mcp
```

MCP 客户端配置示例：

```json
{
  "mcpServers": {
    "eltdx": {
      "command": "eltdx-mcp"
    }
  }
}
```

Windows 上找不到脚本时，可以直接指定安装 `eltdx` 的 Python：

```json
{
  "mcpServers": {
    "eltdx": {
      "command": "C:\\path\\to\\python.exe",
      "args": ["-m", "eltdx.mcp"]
    }
  }
}
```

服务只使用 stdio，不开启 HTTP 端口。标准输出属于 MCP 协议通道，运行日志应写入标准错误。

## 工具列表 {#mcp-tools}

### 7709 行情

| 工具 | 作用 |
| --- | --- |
| `eltdx_quote` | 查询一个或多个证券的行情快照 |
| `eltdx_quote_depth` | 查询五档盘口，单次最多 100 只证券 |
| `eltdx_kline` | 查询一页 K 线，支持周期、复权和锚定日期 |
| `eltdx_minute` | 查询当日或指定日期分时 |
| `eltdx_trades` | 查询一页当日或历史成交明细 |
| `eltdx_call_auction` | 查询当日集合竞价序列 |
| `eltdx_auction_0925` | 查询指定日期 09:25 竞价成交快照 |
| `eltdx_auction_data` | 汇总竞价序列、09:25 快照和实时行情 |

### 汇总与短线指标

| 工具 | 作用 |
| --- | --- |
| `eltdx_stock_profile` | 合并行情、代码表和财务基础信息 |
| `eltdx_shortline_indicators` | 返回交易日安全的 21 个短线指标字段 |

### 题材与 F10

| 工具 | 作用 |
| --- | --- |
| `eltdx_stock_topics` | 查询个股关联的全部题材 |
| `eltdx_topic_stocks` | 查询题材成分股及题材内对比数据 |
| `eltdx_company_profile` | 查询 F10 公司概况 |
| `eltdx_hot_topics` | 查询 F10 热点题材明细 |
| `eltdx_finance_report` | 查询 F10 财务报表 |
| `eltdx_company_news` | 查询 F10 公司资讯或研报 |

### 文档

| 工具 | 作用 |
| --- | --- |
| `eltdx_docs_index` | 返回项目主要文档的 `eltdx://docs/*` resource URI |

服务还发布 `eltdx://docs/*` MCP resources，可直接读取 MCP、API、方法、字段、7709 命令、F10 和 Helper 文档。

## 调用示例

行情快照：

```json
{
  "codes": ["sz000001", "sh600000"],
  "timeout": 3
}
```

前复权日 K：

```json
{
  "code": "sz000001",
  "period": "day",
  "count": 120,
  "adjust": "qfq"
}
```

历史分时：

```json
{
  "code": "sz000001",
  "trading_date": "20260803"
}
```

短线指标：

```json
{
  "codes": ["sz000001", "sh600000"],
  "refresh_stats": false
}
```

题材成分股：

```json
{
  "seed_code": "000034",
  "topic_name": "存储芯片",
  "sort_by": "zdf"
}
```

## 参数和资源边界

| 参数 | 边界 |
| --- | --- |
| 普通行情和汇总工具 `codes` | 每次最多 200 个代码，建议使用 `sz000001`、`sh600000` 格式 |
| 五档盘口 `codes` | `eltdx_quote_depth` 单次最多 100 个代码；`0x0547` 主站超过 100 只会截断，因此 MCP 直接拒绝超限请求 |
| `timeout` | 大于 0 且不超过 120 秒，默认 8 秒 |
| `host` | 可选的单个 7709 主站，例如 `116.205.183.150:7709` |
| K 线 `count` | 每次最多 800 根 |
| 成交 `count` | 每次最多 2000 条 |
| F10 `page_size` | 每次最多 100 条 |

服务按 `host + timeout` 复用 7709 客户端和内存缓存。同一配置只初始化一次，并固定建立 4 个连接槽位；同服务器的多个工具线程会由连接池分配到空闲 TCP 连接，最多 4 个行情请求同时在途，不会让多个线程直接共用同一个 socket。第 5 个及之后的并发请求按 FIFO 等待空闲槽位。最多保留 16 组行情客户端配置；达到上限时关闭并淘汰最久未使用的空闲客户端。服务退出会等待正在执行的行情调用完成，再统一关闭连接。

F10 工具走独立的 `7615/TQLEX` HTTP 调用，不占用 7709 行情客户端槽位。短线指标会使用 7709 行情和统计资源，并遵守交易日及 09:25 就绪检查。
