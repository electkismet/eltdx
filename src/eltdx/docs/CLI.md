# 命令行 `eltdx`

eltdx 自带命令行入口，设计参考 `easy-tdx` CLI：子命令分组、**默认 JSON 输出**、
`--output table` 给人类看。区别是 eltdx **不引入 click / pandas**，
只用标准库 `argparse`，输出走后端无关的 `eltdx.to_jsonable`。

## 入口

安装（wheel/sdist）后：

```bash
eltdx --help
eltdx version
```

未安装、直接跑源码时：

```bash
python -m eltdx --help
PYTHONPATH=src python -m eltdx version
```

`pyproject.toml` 注册的入口点：

| 入口 | 作用 |
| --- | --- |
| `eltdx` | 本文档介绍的主命令 |
| `eltdx-smoke` | 7709 冒烟测试 |
| `eltdx-f10-smoke` | 7615 F10 冒烟测试 |
| `eltdx-mcp` | MCP 服务（见 [MCP.md](MCP.md)） |
| `eltdx-http` | HTTP 网关（见 [HTTP_GATEWAY.md](HTTP_GATEWAY.md)） |

## 全局选项

| 选项 | 说明 |
| --- | --- |
| `-o, --output {json,jsonl,table,csv}` | 输出格式，默认 `json` |
| `--indent N` | JSON 缩进空格数（默认单行紧凑） |
| `--host HOST` | 指定服务器（否则自动探测/故障转移） |
| `--port N` | 端口（`ex`/`macex`/`fund` 默认 7727，`mac` 默认 7709） |
| `--timeout SEC` | 连接/读取超时，默认 15 |

`table` 与 `csv` 会把结果摊平成「一行一条记录」；字典形式的批量返回
（如 `{"sz000001": {...}}`）会自动加上 `code` 列。

## 代码写法

四种子命令都接受以下写法：

```bash
eltdx quote sz000001 sh600519        # 前缀式（推荐）
eltdx quote 000001 600519            # 裸代码，按首位推断市场
eltdx quote "SZ 000001,SH 600519"    # easy-tdx 风格
eltdx mac quote "SZ 000001"          # MAC 命令用市场编号 0=深 1=沪 2=北
```

!!! note "位置参数写在选项后面也可以"
    argparse 原生不支持「选项后跟位置参数」（如 `macex quote --market 33 016947`），
    eltdx 在入口处补齐了这些裸 token，因此下面两种写法等价：
    ```bash
    eltdx fund nav 016947 --market 33
    eltdx fund nav --market 33 016947
    ```
    真正的未知选项（如 `--bogus`）仍然会报错。

## 命令一览

### A 股 7709（需要 native 扩展）

| 命令 | 说明 |
| --- | --- |
| `ping` | 探测服务器 |
| `workday [--date]` | 交易日判断 |
| `quote CODE... [--depth]` | 实时快照 / 五档盘口 |
| `kline CODE [--period] [--count] [--start] [--adjust]` | K 线 |
| `minute CODE [--date]` | 分时 |
| `trades CODE [--date] [--all] [--start] [--count]` | 逐笔成交 |
| `codes [--market] [--kind]` | 代码表（`stocks`/`etfs`/`indices`/`all`/`list`/`count`/`markets`）|
| `finance CODE...` | 专业财报 |
| `money-flow CODE...` | 资金流向日数据 |
| `limits [CODE...] [--date]` | 每日涨跌停价 |

`--period` 取值：`1m` `5m` `15m` `30m` `60m`/`1h`/`hour` `day` `week` `month` `quarter` `year`。

```bash
eltdx ping
eltdx kline sz000001 --period day --count 5 --output table
eltdx quote "sz000001,sh600519" --depth
eltdx codes --market sz --kind etfs --limit 20
eltdx limits sz000001 --date 20261009
```

### 扩展行情 7727

| 命令 | 说明 |
| --- | --- |
| `ex markets` | 市场表（服务端普遍不支持，自动回退本地 `ExMarket` 枚举） |
| `ex count [--market N]` | 全市场标的数 / 某市场标的数 |
| `ex info [--start] [--count]` | 按全局偏移分页取标的 |
| `ex quote --market N CODE` | 单合约行情 |
| `ex quote-list --market N --category C` | 批量行情列表 |
| `ex bars --category C --market N CODE` | K 线（注意第一个参数是 category） |
| `ex minute --market N CODE` / `ex txn --market N CODE` | 分时 / 逐笔 |
| `macex count [--market N]` | 标的数量 |
| `macex instruments --market N` | **代码 + 名称**（推荐用它枚举） |
| `macex list --market N` | 商品列表 |
| `macex quote --market N CODE...` | 批量报价 |
| `macex quote-list --market N` | 排行报价 |
| `macex kline --market N CODE` | K 线 |
| `macex minute` / `macex txn` | 分时 / 逐笔 |

```bash
eltdx ex markets --output table
eltdx ex count --market 33
eltdx ex info --start 40895 --count 5 --output table
eltdx macex instruments --market 33 --count 10 --output table
```

参见 [EXTENDED_MARKET.md](EXTENDED_MARKET.md) 了解各主站的实际能力差异。

### 场外基金（`eltdx fund`）

**场外基金不在 7709 行情里**，必须走扩展行情；这也是 `fund` 子命令存在的原因。

| 命令 | 说明 |
| --- | --- |
| `fund markets` | 列出基金类市场编号 |
| `fund list [--market N] [--start] [--count]` | 枚举基金（代码 + 名称） |
| `fund nav CODE... [--market N]` | 查询净值快照 |
| `fund kline CODE [--market N]` | 净值日线 |

`--market` 默认 `33`（开放式基金），也可用 `34` 货币基金、`49` 香港基金等。

```bash
eltdx fund list --market 33 --count 10 --output table
eltdx fund nav 016947
eltdx fund kline 016947 --count 5 --output table
```

`fund nav 016947` 的实际输出：

```json
[{"market": 33, "code": "016947", "name": "国泰利安中短债A",
  "fields": {"pre_close": 1.0924, "open": 1.0924, "high": 1.0924,
             "low": 1.0924, "close": 1.0924, "vol": 0, "amount": 0.0}}]
```

基金没有成交量，`vol`/`amount` 恒为 0，价格字段即**单位净值**。

### MAC 协议（7709 MAC 服务器）

`mac quote` `mac kline` `mac ranking` `mac board-list` `mac server-info`
`mac symbol-info` `mac minute` `mac txn` `mac auction` `mac unusual` `mac goods-list`

```bash
eltdx mac quote "SZ 000001"
eltdx mac board-list --count 5 --output table
eltdx mac server-info
eltdx mac kline --market 0 600000 --count 3
```

!!! warning "`mac kline` 可能被服务端拒绝"
    部分 MAC 服务器对 `symbol_bar` 只回 1 字节，两库都会抛出
    `ProtocolError: symbol_bar header: 数据不足`。这是服务端行为，不是 CLI 的问题。

### 本地行情文件

| 命令 | 说明 |
| --- | --- |
| `offline day` | `.day` 日线 |
| `offline min` / `offline 5min` | `.lc1` / `.lc5` |
| `offline ex-day` | 扩展市场日线 |
| `offline block` / `offline industry` | 板块 / 行业配置 |
| `offline gbbq` / `offline financial` | 权息数据 / 历史财务 |

用 `--file PATH` 直接指定文件，或用 `--vipdoc DIR --market N --code CODE` 自动定位：

```bash
eltdx offline day --file path/to/sz000001.day --count 5 --output table
eltdx offline day --vipdoc D:/new_tdx/vipdoc --market 0 --code 000001 --last
eltdx offline gbbq --file path/to/gbbq
```

## 输出示例

```console
$ eltdx fund list --market 33 --count 3 --output table
+----------+--------+--------+----------+------+
| category | market | code   | name     | desc |
+----------+--------+--------+----------+------+
| 8        | 33     | 000001 | 华夏成长   |      |
| 8        | 33     | 000003 | 中海可转债A |      |
| 8        | 33     | 000004 | 中海可转债C |      |
+----------+--------+--------+----------+------+
(3 行)

$ eltdx fund kline 016947 --count 2 --output csv
datetime,open,high,low,close,vol,amount,float_shares
2026-10-08T00:00:00,1.0924,1.0924,1.0924,1.0924,0,0,0
2026-10-09T00:00:00,1.0924,1.0924,1.0924,1.0924,0,0,0

$ eltdx macex instruments --market 33 --count 2 --output jsonl
{"category": 8, "market": 33, "code": "000001", "name": "华夏成长", "desc": ""}
{"category": 8, "market": 33, "code": "000003", "name": "中海可转债A", "desc": ""}
```

## 退出码

| 码 | 含义 |
| --- | --- |
| 0 | 成功（包括「查到了 0 条」） |
| 1 | 业务/网络错误，消息写到 stderr（`错误: TransportError: ...`） |
| 2 | 参数错误（argparse） |
| 130 | Ctrl-C |

## 与 Python API 的对应

| CLI | Python |
| --- | --- |
| `eltdx quote` | `client.quotes.get_snapshots` / `get_depth` |
| `eltdx kline` | `client.bars.get` |
| `eltdx codes` | `client.codes.*` |
| `eltdx fund list` | `MacExClient.instruments` |
| `eltdx fund nav` | `MacExClient.goods_quotes` |
| `eltdx fund kline` | `MacExClient.goods_kline` |
| `eltdx ex *` | `ExTdxClient.*` |
| `eltdx mac *` | `MacClient.*` |
| `eltdx macex *` | `MacExClient.*` |
| `eltdx offline *` | `eltdx.offline.*` |

CLI 只做参数解析与输出渲染，不复制任何业务逻辑；需要更细的控制（并发、批量、
原始字节）时请直接用 Python API。
