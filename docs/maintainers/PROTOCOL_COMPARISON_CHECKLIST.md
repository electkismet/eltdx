# 协议实现核对清单

这份文档记录当前 `eltdx` 协议实现的覆盖范围、置信度、派生 helper 边界，以及发布前还需要继续关注的地方。

它不是“历史来源说明”，而是**当前实现状态清单**。

## 1. 核对依据

当前结论主要来自以下 4 类证据：

- `src/eltdx/client.py`
- `src/eltdx/transport/*.py`
- `src/eltdx/protocol/*.py`
- `artifacts/live_validation_20260307_075132/summary.json`

补充依据：

- `python -m pytest -q`
- 各类 smoke / 导出脚本
- 在线验证样本文件

## 2. 当前整体状态

当前 `eltdx` 已经处于：**主链路可用、字段模型稳定、语义边界清晰** 的状态。

最新基线：

- 在线验证：`43 passed / 0 failed`
- 分时实时路径与历史路径已拆开验证
- 自动分批、双长连接、上下文管理器已跑通
- 时间字段已统一转成 Python 原生 `date` / `datetime`

## 3. 核心协议覆盖（A）

以下属于当前项目的核心公开能力，已经完成协议实现与在线验证：

| 能力 | Python 入口 | 置信度 | 说明 |
| --- | --- | --- | --- |
| 生命周期 | `connect()` / `close()` / `with` | A | 连接池、线程池、资源释放已稳定 |
| 市场计数 | `get_count()` | A | 语义已明确为代码表条目数 |
| 代码表分页 | `get_codes()` / `get_codes_all()` | A | 能稳定返回底层代码表 |
| 行情快照 | `get_quote()` | A | 自动分批 + 双连接分发已验证 |
| 逐笔 | `get_trades()` / `get_trades_all()` | A | 实时 / 历史都已验证 |
| K 线 | `get_kline()` / `get_kline_all()` | A | 股票 / 指数场景均可用 |
| 集合竞价 | `get_call_auction()` | A | 核心字段与 raw 调试已验证 |
| 分时 | `get_minute()` | A | 无日期走实时路径 |
| 历史分时 | `get_history_minute()` | A | 传日期走历史路径 |
| GBBQ | `get_gbbq()` | A | 原始公司行为记录已验证 |

## 4. 实用 helper（B）

以下能力是面向实际调用体验的实用封装，结果可用，但要理解它们是**业务 helper**：

| 能力 | Python 入口 | 置信度 | 说明 |
| --- | --- | --- | --- |
| 股票数量 | `get_stock_count()` | B | 在完整代码表基础上做过滤统计 |
| A 股数量 | `get_a_share_count()` | B | 比股票数量更严格，排除 B 股 |
| 股票 / ETF / 指数清单 | `get_stock_codes_all()` / `get_etf_codes_all()` / `get_index_codes_all()` | B | 适合拉行情，不等于官方分类总表 |
| 频率兼容别名 | `get_kline(freq, code)` 等 | B | 兼容旧调用习惯，但推荐 `get_kline(code, freq)` |
| `kind="index"` 扩展字段 | `up_count` / `down_count` | B | 在指数型 K 线里更有意义 |

## 5. 派生能力（C）

以下不是独立协议，而是在核心接口之上做的业务加工：

| 能力 | Python 入口 | 置信度 | 说明 |
| --- | --- | --- | --- |
| XDXR | `get_xdxr()` | C | 由 `get_gbbq()` 过滤并转换得到 |
| 股本变化 | `get_equity_changes()` / `get_equity()` | C | 由 GBBQ 业务化处理得到 |
| 换手率 | `get_turnover()` | C | 基于股本记录与成交量计算 |
| 复权因子 | `get_factors()` | C | 基于日 K 与公司行为记录构造 |
| 复权 K 线 | `get_adjusted_kline()` / `get_adjusted_kline_all()` | C | K 线 + 因子加工结果 |
| 逐笔转分钟 K | `get_trade_minute_kline()` / `get_history_trade_minute_kline()` | C | 逐笔聚合结果 |
| 服务层对象 | `CodesService` / `WorkdayService` / `GbbqService` | C | 面向调用便利的内存缓存封装 |

## 6. 当前明确边界

以下内容在文档里已经明确做了保守说明：

- `get_count()` 不是股票总数接口
- `get_codes*()` 不是“官方证券分类主表”
- `Quote.active1` / `active2` / `rate` 等字段不做过强业务定义
- `inside_dish` / `outer_disc` 当前按内盘 / 外盘口径保守解释
- `multiple` / `decimal` 是协议换算参数，不建议业务层直接依赖

## 7. 发布前仍建议继续补强的点

### 7.1 多周期 K 线抽检

建议继续增加在线抽检：

- `5m`
- `15m`
- `30m`
- `60m`
- `week`
- `month`
- `quarter`
- `year`

### 7.2 多品种样本抽检

建议继续覆盖：

- B 股
- 北交所样本
- ETF
- 指数
- 更多非股票代码表条目

### 7.3 文档口径持续收敛

后续若某些字段的业务语义进一步坐实，应优先同步：

- `FIELD_REFERENCE.md`
- `FIELD_SEMANTICS_LOCKDOWN.md`
- `README.md`

## 8. 最终目标

目标不是把每个底层字节都包装成“听起来很确定的业务术语”，而是：

- 能稳定联网获取数据
- 能稳定解析成 Python dataclass
- 能明确告诉用户哪些字段已经可信、哪些字段应保守理解
