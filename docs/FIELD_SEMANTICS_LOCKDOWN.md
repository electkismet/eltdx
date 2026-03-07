# 字段语义三重锁定

这份文档专门记录：**实现源码 + 在线实测 + 中文业务口径** 三重锁定之后，哪些字段已经可以放心解释，哪些字段还要谨慎。

目标不是“把所有字段都硬解释一遍”，而是把**证据链**写清楚。

## 1. 锁定方法

每个字段都尽量同时满足 3 个条件：

1. **实现源码**：当前项目源码里能找到字段命名、注释、映射关系或计算逻辑
2. **在线实测**：当前 `eltdx` 在线导出里能看到真实返回值
3. **中文口径**：能沉淀成适合 README / API 文档 / GitHub 展示的中文解释

只有 1 和 2 成立，但 3 仍然模糊时，就不把它写成“完全确定的业务指标”。

## 2. 置信度说明

- `A`：实现逻辑明确，在线实测已对上，中文业务口径稳定
- `B`：有字段命名或处理逻辑，在线实测也对上，但业务层解释仍需保守
- `C`：字段能稳定解析，但源码侧也没有足够语义，不建议强解释

## 3. 已锁定字段

### 3.1 代码表

| Python 字段 | 实现证据 | Python 实测 | 最终中文口径 | 置信度 |
| --- | --- | --- | --- | --- |
| `SecurityCode.multiple` | `src/eltdx/protocol/model_code.py` 直接解析该字段 | `multiple = 100` | 倍数 | `B` |
| `SecurityCode.decimal` | `src/eltdx/protocol/model_code.py` 直接解析该字段 | `decimal = 2` | 小数位 | `B` |
| `SecurityCode.last_price` | `src/eltdx/protocol/model_code.py` 解析后输出浮点价格 | `last_price = 1491.0` | 最近价字段 | `B` |

证据：

- 源码：`src/eltdx/protocol/model_code.py`
- 实测：`artifacts/live_validation_20260307_075132/codes_page_sz_start0_limit20.json`

说明：

- `multiple` 和 `decimal` 的存在意义是明确的，但业务层通常不会直接依赖它们来展示行情。

### 3.2 快照 `Quote`

| Python 字段 | 实现证据 | Python 实测 | 最终中文口径 | 置信度 |
| --- | --- | --- | --- | --- |
| `active1` | `src/eltdx/protocol/model_quote.py` 直接读取并保留 | `3867` | 活跃度字段 1 | `B` |
| `active2` | `src/eltdx/protocol/model_quote.py` 直接读取并保留 | `3867` | 活跃度字段 2 | `B` |
| `server_time_raw` | `src/eltdx/protocol/model_quote.py` 原始整数时间字段 | `15330719` | 服务端原始时间整数 | `A` |
| `server_time` | `src/eltdx/protocol/model_quote.py` 经 `decode_quote_datetime()` 解析 | `2026-03-07T15:33:07.190000+08:00` | 服务端时间 | `A` |
| `total_hand` | `src/eltdx/protocol/model_quote.py` 直接解析累计量 | `476576` | 总手 | `A` |
| `intuition` | `src/eltdx/protocol/model_quote.py` 直接保留量能字段 | `6768` | 现量 / 当前成交量 | `A` |
| `amount` | `src/eltdx/protocol/model_quote.py` 解析累计金额 | `514733536.0` | 成交额 | `A` |
| `inside_dish` | `src/eltdx/protocol/model_quote.py` 直接保留 | `206012` | 内盘 | `B` |
| `outer_disc` | `src/eltdx/protocol/model_quote.py` 直接保留 | `270565` | 外盘 | `B` |
| `rate` | `src/eltdx/protocol/model_quote.py` 解析比率字段 | `0.0` | 涨速 | `B` |

证据：

- 源码：`src/eltdx/protocol/model_quote.py`
- 实测：`artifacts/live_validation_20260307_075132/quote_single_sz000001.json`

说明：

- `active1` / `active2` 虽然命名已固定，但没有进一步业务计算说明，因此目前定为 `B`。
- `inside_dish` / `outer_disc` 当前按内盘/外盘口径使用，但不把它们写成对第三方软件逐项一致的强结论。

### 3.3 逐笔 `Trade`

| Python 字段 | 实现证据 | Python 实测 | 最终中文口径 | 置信度 |
| --- | --- | --- | --- | --- |
| `time` | `src/eltdx/protocol/model_trade.py` 按分钟时间恢复 | `2026-03-07T14:54:00+08:00` | 逐笔成交时间 | `A` |
| `price` | `src/eltdx/protocol/model_trade.py` 价格差分恢复 | `10.83` | 成交价 | `A` |
| `volume` | `src/eltdx/protocol/model_trade.py` 直接解析量字段 | `63` | 成交量（手） | `A` |
| `status` | `src/eltdx/protocol/model_trade.py` 状态码 + `_status_to_side()` | `0` | 成交方向状态码 | `A` |
| `side` | `_status_to_side()` 映射逻辑 | `buy` | 买卖方向 | `A` |
| `order_count` | 实时逐笔直接解析，历史逐笔为空 | `7` | 单数 / 聚合单数 | `A` |

证据：

- 源码：`src/eltdx/protocol/model_trade.py`
- 实测：`artifacts/live_validation_20260307_075132/trades_live_page_sz000001.json`

### 3.4 集合竞价 `CallAuction`

| Python 字段 | 实现证据 | Python 实测 | 最终中文口径 | 置信度 |
| --- | --- | --- | --- | --- |
| `match` | `src/eltdx/protocol/model_call_auction.py` 直接解析 | `19` | 撮合量 | `A` |
| `unmatched` | `src/eltdx/protocol/model_call_auction.py` 有符号值转绝对值 | `24` | 未撮合量 | `A` |
| `flag` | 当前实现：若未撮合原始值为负则 `-1`，否则 `1` | `-1` | 未撮合方向：`1=买未撮合`，`-1=卖未撮合` | `A` |

证据：

- 源码：`src/eltdx/protocol/model_call_auction.py`
- 实测：`artifacts/live_validation_20260307_075132/call_auction_sz000001.json`

### 3.5 GBBQ / XDXR / Equity / Factor

| Python 字段 | 实现证据 | Python 实测 | 最终中文口径 | 置信度 |
| --- | --- | --- | --- | --- |
| `category_name` | `src/eltdx/protocol/model_gbbq.py` 内置 `CATEGORY_NAMES` | `1 -> 除权除息`，`5 -> 股本变化` | 事件类别名称 | `A` |
| `XdxrItem.fenhong` | `filter_xdxr_items()` 中由 `c1` 映射并四舍五入 | `0.0` | 分红 | `A` |
| `XdxrItem.peigujia` | `filter_xdxr_items()` 中由 `c2` 映射并四舍五入 | `3.56` | 配股价 | `A` |
| `XdxrItem.songzhuangu` | `filter_xdxr_items()` 中由 `c3` 映射并四舍五入 | `0.0` | 送转股 | `A` |
| `XdxrItem.peigu` | `filter_xdxr_items()` 中由 `c4` 映射并四舍五入 | `1.0` | 配股 / 配股比例 | `A` |
| `EquityItem.float_shares` | `filter_equity_items()` 中由 `c3` 转出 | `26500000` | 流通股本 | `A` |
| `EquityItem.total_shares` | `filter_equity_items()` 中由 `c4` 转出 | `48500170` | 总股本 | `A` |
| `FactorItem.qfq_factor` | `build_factor_response()` 与复权计算逻辑 | `0.0031807693670508697` | 前复权因子 | `A` |
| `FactorItem.hfq_factor` | `build_factor_response()` 与复权计算逻辑 | `1.0` | 后复权因子 | `A` |

证据：

- 源码：
  - `src/eltdx/protocol/model_gbbq.py`
  - `src/eltdx/equity.py`
  - `src/eltdx/adjustment.py`
- 实测：
  - `artifacts/live_validation_20260307_075132/gbbq_sz000001.json`
  - `artifacts/live_validation_20260307_075132/xdxr_sz000001.json`
  - `artifacts/live_validation_20260307_075132/equity_changes_sz000001.json`
  - `artifacts/live_validation_20260307_075132/factors_sz000001.json`

## 4. 当前仍保持保守口径的字段

以下字段虽然已经能稳定解析，但仍不建议写成过强的业务结论：

| 字段 | 当前中文口径 | 原因 |
| --- | --- | --- |
| `Quote.active1` | 活跃度字段 1 | 只有字段命名，没有进一步业务算法说明 |
| `Quote.active2` | 活跃度字段 2 | 只有字段命名，没有进一步业务算法说明 |
| `Quote.inside_dish` | 内盘 | 当前按内盘口径使用，但不承诺与第三方软件逐项一致 |
| `Quote.outer_disc` | 外盘 | 当前按外盘口径使用，但不承诺与第三方软件逐项一致 |
| `Quote.rate` | 涨速 | 当前按涨速口径保守解释，样本常见 `0.0` |
| `SecurityCode.multiple` | 倍数 | 作用方向明确，但业务层通常不直接消费 |
| `SecurityCode.decimal` | 小数位 | 作用方向明确，但业务层通常不直接消费 |

## 5. 这份锁定表怎么用

建议你以后看字段说明时按这个顺序判断：

1. 先看 [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md) 的中文解释
2. 如果字段比较底层，再看这份表的“实现证据 + 置信度”
3. 真要继续深挖，再直接去看 `src/eltdx/` 里的对应实现

这样你就能分清：

- 哪些字段已经是“可以写进 README 的正式口径”
- 哪些字段只是“协议层能返回，但业务意义暂时保守”
