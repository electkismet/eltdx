# 从 easy_tdx 迁移到 eltdx

本文对照 **easy_tdx 1.20.7** 与 eltdx，列出把业务代码从 `easy_tdx` 换到 `eltdx`
时需要改动的部分。每一节都标了**属于哪一类**：

| 标记 | 含义 |
| --- | --- |
| 🔁 **改名** | 功能一一对应，只是名字不同，照表替换即可 |
| 🎯 **有意不同** | eltdx 刻意改了设计，不会改回去；需要调整调用方 |
| ⏳ **尚未提供** | eltdx 目前没有对应能力，需评估影响 |
| ✅ **已对齐** | 曾经不同，现已一致 |

模块布局本身就是对齐的，`easy_tdx.ex.client` ↔ `eltdx.ex.client`、
`easy_tdx.ex.mac_client` ↔ `eltdx.ex.mac_client`、`easy_tdx.mac.client` ↔
`eltdx.mac.client`、`easy_tdx.models.enums` ↔ `eltdx.models.enums`。

---

## 1. 返回值类型 🎯 有意不同

这是**最大的断裂面**，也是迁移时最需要成片改动的地方：

| | easy_tdx | eltdx |
| --- | --- | --- |
| K 线 / 分时 / 逐笔 / 板块行情 | `pandas.DataFrame` | `list[dataclass]` |

eltdx **不引入 pandas 依赖**（运行时零依赖），所以所有原本返回 `DataFrame` 的方法
现在返回 dataclass 列表。需要用 pandas 时自行转换：

```python
import pandas as pd
from eltdx.ex.client import ExTdxClient

with ExTdxClient.from_best_host() as client:
    bars = client.get_instrument_bars(31, "00700", count=100)
    df = pd.DataFrame(dataclasses.asdict(bar) for bar in bars)
```

字段名与 easy_tdx 的 DataFrame 列名保持一致（例如 `open`/`high`/`low`/`close`）。

---

## 2. 离线文件读写 🔁 改名

`eltdx.offline` 的功能是齐的（读、写、增量追加都在），只是统一成了**单数**命名。
下表每一行都是实测对照过的：

| easy_tdx | eltdx | 说明 |
| --- | --- | --- |
| `read_daily_bars` | `read_daily_bar` | 🔁 |
| `read_ex_daily_bars` | `read_ex_daily_bar` | 🔁 |
| `read_lc_min_bars` | `read_min_bar` | 🔁 lc1 分钟线 |
| `read_5min_bars` | `read_5min_bar` | 🔁 lc5 五分钟线 |
| `read_block_dat` | `read_block` | 🔁 |
| `read_customer_blocks` | `read_customer_block` | 🔁 |
| `append_daily_bars` | `append_daily_bar` | 🔁 |
| `append_ex_daily_bars` | `append_ex_daily_bar` | 🔁 |
| `append_lc_min_bars` | `append_min_bar` | 🔁 |
| `append_5min_bars` | `append_5min_bar` | 🔁 |
| `get_last_lc_min_bar_datetime` | `get_last_min_bar_datetime` | 🔁 签名完全相同 |
| `encode_lc_min_bar` | `encode_min_bar` | 🔁 |
| `sync_daily_bars_from_security_bars` | `append_daily_bar` | 🔁 上游本身就是 `append_daily_bars` 的别名 |
| `sync_ex_daily_bars` | `append_ex_daily_bar` | 🔁 上游本身就是 `append_ex_daily_bars` 的别名 |

两个需要留意的**入参模型**差异（属 🎯）：

* `read_daily_bars` / `append_daily_bars` 在 easy_tdx 里收发 `SecurityBar`，
  eltdx 用 `DailyBar`。
* `read_lc_min_bars` / `read_5min_bars` / `append_lc_min_bars` 收发 `SecurityBar`，
  eltdx 用 `MinBar`。

eltdx 额外提供 easy_tdx 没有的：`write_daily_bar` / `write_min_bar` /
`write_ex_daily_bar` / `write_5min_bar`（整文件覆盖写）、`detect_security_type`、
`security_coefficients`、`parse_financial_dat`、`parse_tdxhy_cfg` 等。

---

## 3. 扩展行情模型 ✅ 已对齐

`eltdx.ex.models` 下 6 个记录级模型**现在都带 `_raw`**，与 easy_tdx 同名同语义
（该条记录在响应体中的原始字节切片），7 个构造点全部填充：

`ExInstrumentBar`、`ExInstrumentInfo`、`ExInstrumentQuote`、`ExMarketInfo`、
`ExMinuteBar`、`ExTransactionRecord`。

字段声明与 easy_tdx 一致——`repr=False`、`compare=False`、默认 `b""`，
所以**不参与相等比较、不出现在 `repr`、也不影响既有构造方式**。

---

## 4. `Market` 枚举 ✅ 已对齐

```python
from eltdx import Market          # easy_tdx: from easy_tdx.models.enums import Market
Market.SZ == 0; Market.SH == 1; Market.BJ == 2
Market.SH.code == "sh"
```

eltdx 内部仍以小写字符串为基准，`Market` 与 `"sz"/"sh"/"bj"` 可互换传给任何
`market` 参数。

---

## 5. MAC 客户端 ✅ 已补齐 / ⏳ 仍缺

| easy_tdx `MacClient` | eltdx |
| --- | --- |
| `from_best_host(...)` | ✅ 有（不含下面两个参数） |
| `ping_all(...)` | ✅ 有 |
| `ensure_connected()` | ✅ 有（用真实探测，不是只看标志位） |
| `get_stock_kline_with_indicators(...)` | ⏳ 尚未提供（依赖第 7 节的 `indicator` 模块） |

**参数差异 🎯**：easy_tdx 的 `from_best_host` 还有 `auto_reconnect` 与
`heartbeat_interval`，eltdx **没有**——因为 `MacConnection` 本身不实现自动重连与
心跳。eltdx 选择不提供这两个参数，而不是提供会被静默忽略的参数。

`eltdx.mac` 另外导出了底层 `ping_mac_all` / `ping_mac_host`。

---

## 6. 若干参数与语义差异 🎯

| 项目 | 差异 |
| --- | --- |
| `goods_quotes_list(..., sort_type, sort_order)` | ✅ 现可**位置传参**并接受普通 `int`。注意这两个参数在 easy_tdx 里也**只是预留、不生效**（底层 `GoodsListCmd` 请求体只有 `market/start/count`），eltdx 同样不生效 |
| `get_instrument_bars` / `get_history_instrument_bars_range` / `get_stock_kline` / `bars.get` 的 `bar_time` | ✅ **已提供**（默认 `"start"` = 通达信原始；`"end"` = 右端点，给分钟级 bar 加上一个周期时长以对齐 Tushare/同花顺）。日线及以上不受影响；`get_history_instrument_bars_range` 与 easy_tdx 一样**不支持 `"end"`**（只按日期范围查、不知道周期时长），会抛 `ValueError`；非法取值同样 fail-fast。三条路径的进位语义与 easy_tdx 逐条对拍一致（扩展行情按 `hour/minute` 的 `%24` 回绕；MAC 按真实 `timedelta`、可跨日；7709 按 `%24` 回绕） |
| 多分钟周期 `"{N}m"` | 🎯 **eltdx 正确，easy_tdx 有误**。7709 的 `period_raw=8` 是 `"{N}m"` 多分钟（N = `period_param_raw`），eltdx 据此算出 N 分钟；而 easy_tdx 的 `KlineCategory.MIN_3 = 8` 把它当成**固定 3 分钟**，且完全忽略周期参数。**实盘判定**：`period="13m"` → `raw=8/param=13`，相邻 bar 实测间隔就是 **13 分钟**（`13:49 / 14:02 / 14:15`），故以 eltdx 为准 |
| `ex.commands.setup` / `client.send_setup()` | 🎯 **已移除**。easy_tdx 里这个常量被定义但**从未被任何代码引用**；eltdx 曾把它提升为公开 API，实测会让服务器在回应一帧常量后不再响应任何请求，故删除 |
| 资金流 `get_history_fund_flow` | ⏳ eltdx 的 `money_flow.daily()` 只是底层 `0x0ffc` 直连接口的封装，**没有** easy_tdx 的「直连为空时用日 K + 历史逐笔重算」回退，也没有空数据时的逐台主机故障转移。正常股票直连是**有数据**的，故该回退属韧性特性 |

---

## 7. 整块尚未提供的模块 ⏳

| 模块 | easy_tdx 内容 | eltdx 现状 |
| --- | --- | --- |
| 异步客户端 | `AsyncTdxClient` / `AsyncExTdxClient` / `AsyncMacClient` / `AsyncMacExClient` / `AsyncUnifiedTdxClient` | 尚未提供，只有同步版 |
| 指标计算 | `indicator` 模块：`MyTT` / `compute_indicators` / `list_indicators` / `IndicatorSpec` | 尚未提供；`get_stock_kline_with_indicators` 也依赖它 |

这两块是**新增功能**而非接口改名，迁移前需确认是否用到了。

---

## 8. 迁移自查清单

1. 把所有 `easy_tdx` 导入替换为对应的 `eltdx` 路径（模块布局相同）。
2. 凡是原本拿 `DataFrame` 的地方，改成消费 dataclass 列表（第 1 节）。
3. 离线读写按第 2 节的表改名。
4. 确认没有用到第 7 节的两个模块。
5. 确认没有依赖 `send_setup()`（已移除）。
6. 若用到 `get_history_fund_flow` 的空数据回退行为，按第 6 节评估。
