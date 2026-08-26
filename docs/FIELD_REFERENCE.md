# 字段手册

这份文档解释常用返回模型的字段含义。按调用方法查参数和返回字段时看 [METHOD_REFERENCE.md](METHOD_REFERENCE.md)。底层协议字段和命令号见 [COMMANDS_7709.md](COMMANDS_7709.md)，历史字段对照见 [FIELD_MIGRATION.md](FIELD_MIGRATION.md)。

## 通用约定

| 约定            | 说明                                   |
| ------------- | ------------------------------------ |
| `exchange`    | 市场前缀，常见为 `sh` / `sz` / `bj`          |
| `code`        | 六位代码，例如 `000001`                     |
| `full_code`   | 完整代码，等于 `exchange + code`            |
| `*_raw`       | 协议原始值或原始片段，保留给排查使用                   |
| `record_hex`  | 单条记录原始十六进制                           |
| `raw_payload` | 当前响应 payload 原始 bytes                |
| `*_milli`     | 毫厘价格，通常 `price = price_milli / 1000` |

## SecurityCode

代码表单条记录。

| 字段                     | 含义                                |
| ---------------------- | --------------------------------- |
| `exchange`             | 市场                                |
| `market_id`            | 市场编号，`0=sz`、`1=sh`、`2=bj`         |
| `code`                 | 六位代码                              |
| `name`                 | 名称                                |
| `multiple`             | 协议价格换算相关倍数                        |
| `decimal`              | 小数位                               |
| `previous_close_price` | 昨收参考价                             |
| `volume_ratio_base`    | 量比相关基础值                           |
| `category`             | 派生品种分类，例如 `a_share`、`index`、`etf` |
| `category_reason`      | 分类命中规则说明                          |
| `board`                | 派生板块，例如主板、创业板、科创板、北交所             |
| `board_reason`         | 板块命中规则说明                          |
| `full_code`            | 完整代码属性                            |

## QuoteSnapshot

批量快照返回单条行情。

| 字段                 | 含义          |
| ------------------ | ----------- |
| `last_price`       | 最新价         |
| `pre_close_price`  | 昨收价         |
| `open_price`       | 今开          |
| `high_price`       | 最高          |
| `low_price`        | 最低          |
| `total_hand`       | 总成交量，单位手    |
| `current_hand`     | 现手          |
| `amount`           | 成交额         |
| `inside_dish`      | 内盘          |
| `outer_disc`       | 外盘          |
| `open_amount_yuan` | 开盘金额，单位元    |
| `buy_levels`       | 买盘档位；`client.helpers.full_quotes()` 补齐五档，直接 `get_snapshots()` 为已确认一档 |
| `sell_levels`      | 卖盘档位；`client.helpers.full_quotes()` 补齐五档，直接 `get_snapshots()` 为已确认一档 |
| `change`           | 涨跌额，派生字段    |
| `change_pct`       | 涨跌幅百分比，派生字段 |
| `sum_buy_vol`      | 五档买量合计，派生字段 |
| `sum_sell_vol`     | 五档卖量合计，派生字段 |

## LegacyQuote

`0x053e` 旧版批量行情返回记录。

| 字段 | 含义 |
| --- | --- |
| `last_price` / `pre_close_price` | 最新价 / 昨收价 |
| `open_price` / `high_price` / `low_price` | 今开 / 最高 / 最低 |
| `total_hand` / `current_hand` | 总成交量 / 现手 |
| `amount` / `amount_raw` | 成交额解析值 / 原始值 |
| `inside_dish` / `outer_disc` | 内盘 / 外盘 |
| `buy_levels` / `sell_levels` | 五档买盘 / 五档卖盘 |
| `trading_status_raw` / `trading_status_hex` | 交易状态原始值 / 十六进制文本 |
| `tail_metrics_raw` | 四个尾部指标原始值 |
| `rise_speed_raw` / `active2` | 可选旧版尾部字段 |
| `record_hex` | 单条记录原始十六进制 |

## CategoryQuoteRecord

分类行情列表记录，对应按板块/类别排序拉取。

| 字段                                        | 含义                    |
| ----------------------------------------- | --------------------- |
| `last_price` / `pre_close_price`          | 最新价 / 昨收              |
| `open_price` / `high_price` / `low_price` | 开高低                   |
| `total_hand` / `current_hand`             | 总量 / 现量               |
| `amount`                                  | 成交额                   |
| `inside_dish` / `outer_disc`              | 内外盘                   |
| `bid1` / `ask1`                           | 买一 / 卖一价格             |
| `bid_vol1` / `ask_vol1`                   | 买一 / 卖一量              |
| `rise_speed`                              | 涨速                    |
| `short_turnover`                          | 短周期换手口径字段             |
| `min2_amount`                             | 近 2 分钟金额口径字段          |
| `opening_rush`                            | 开盘冲击口径字段              |
| `vol_rise_speed`                          | 量增速                   |
| `depth`                                   | 深度字段                  |
| `locked_amount`                           | 买一价格 * 买一量 * 100，派生字段 |
| `record_hex`                              | 单条记录原始十六进制            |

## KlineSeries / KlineBar

K 线响应和单根 K 线。

| 字段                                | 含义                        |
| --------------------------------- | ------------------------- |
| `period_name`                     | 周期，例如 `day`、`1m`          |
| `adjust_mode`                     | 复权模式，`none`、`qfq`、`hfq` 等 |
| `anchor_date`                     | 定点复权日期                    |
| `bars`                            | K 线列表                     |
| `time`                            | K 线时间                     |
| `open` / `high` / `low` / `close` | 开高低收                      |
| `last_close_price_milli`          | 上一根收盘毫厘价                  |
| `volume_lots`                     | 成交量，单位手                   |
| `amount`                          | 成交额                       |
| `up_count` / `down_count`         | 上涨/下跌家数，指数类样本可能有          |
| `raw_payload`                     | 响应 payload                |
| `record_hex`                      | 单条 K 线记录原文                |

## FileContentChunk

`0x06b9` 服务器文件读取结果。

| 字段 | 含义 |
| --- | --- |
| `path` | 服务器文件路径 |
| `offset` | 本次读取偏移 |
| `request_size` | 请求字节数 |
| `chunk_len` | 实际返回字节数 |
| `content` | 文件块原始 bytes |
| `raw_payload` | 原始响应 payload |
| `is_last` | 返回长度小于请求长度时为 `True` |

## MinuteSeries / MinutePoint

分时响应和单点。

| 字段             | 含义           |
| -------------- | ------------ |
| `trading_date` | 交易日          |
| `points`       | 分时点          |
| `prev_close`   | 昨收           |
| `open_price`   | 今开           |
| `index`        | 分时序号         |
| `time_label`   | 时间文本         |
| `price`        | 当日分时价格       |
| `avg_price`    | 均价           |
| `volume`       | 分钟成交量，单位手    |
| `volume_sum`   | 分时成交量合计，派生字段 |

## TradePage / TradeTick

成交明细响应和单条混合事件。`0x0fc5`（当前）与 `0x0fc6`（历史）可能同时返回普通成交、竞价快照、09:25 与 15:00 正式撮合，以及 `status=5` 盘后固定价格成交。

| 字段                  | 含义                            |
| ------------------- | ----------------------------- |
| `trading_date`      | 交易日                           |
| `start`             | 请求起始位置                        |
| `request_count`     | 请求条数                          |
| `ticks`             | 服务器原始混合记录，不丢弃 `status=8` 竞价快照                          |
| `actual_trades`     | 排除 `status=8` 后的真实成交，包含 09:25、15:00 和盘后固定价格成交 |
| `after_hours_trades` | 15:05-15:30、`status=5` 的盘后固定价格成交 |
| `auction_snapshots` | `status=8` 竞价快照                |
| `opening_matches`   | 09:25 正式开盘撮合                  |
| `time_label`        | 时间文本                          |
| `trade_datetime`    | 成交时间                          |
| `price`             | 成交价                           |
| `volume`            | 原始数量字段；真实成交时是成交量，`status=8` 时不赋予竞价数量语义 |
| `order_count`       | 原始笔数字段；`status=8` 时不赋予竞价未匹配量语义 |
| `event_kind`        | `trade`、`auction_snapshot`、`opening_match` |
| `is_auction_snapshot` | 是否为竞价快照                    |
| `is_opening_match`   | 是否为 09:25 正式撮合              |
| `is_trade`           | 是否为普通成交                     |
| `is_actual_trade`    | 是否为真实成交；仅 `status=8` 返回 `False` |
| `is_after_hours_fixed_price` | 是否为 15:05-15:30、`status=5` 的盘后固定价格成交 |
| `auction_matched_volume` | 成交明细不推断竞价数量，固定为 `None` |
| `auction_unmatched_signed_volume` / `auction_unmatched_volume` | 成交明细不推断竞价未匹配量，固定为 `None` |
| `side`              | 方向，`buy` / `sell` / `neutral` |
| `trade_amount_yuan` | `price * volume * 100`；只应对 `is_actual_trade=True` 的记录作为成交额使用 |
| `has_more`          | 单页结果是否可能还有下一页，派生字段                |

分类规则：`status_raw == 8` 为非成交的 `auction_snapshot`；时间为 `09:25` 且不是 `status=8` 为真实的 `opening_match`；15:00 的非 `status=8` 记录是正式收盘撮合；15:05-15:30 的 `status=5` 是盘后固定价格真实成交。成交明细保留竞价快照的原始 `volume` / `order_count`，但不把它们解释为虚拟匹配量或未匹配量；完整竞价数量使用 `client.auctions.series()`。

## AuctionSeries / AuctionPoint

集合竞价过程快照。

| 字段                         | 含义         |
| -------------------------- | ---------- |
| `trading_date`             | 请求的历史日期；当日请求为 `None` |
| `points`                   | 竞价明细记录     |
| `time_label`               | 时间         |
| `price`                    | 竞价价格       |
| `matched_volume`           | 虚拟成交量      |
| `unmatched_volume`         | 未匹配量       |
| `unmatched_direction_raw`  | 未匹配方向原始值   |
| `matched_amount_estimated` | 估算成交额，派生字段 |

## CapitalChangeBlock / CapitalChangeRecord

广义权息 / 股本变迁事件。

| 字段 | 含义 |
| --- | --- |
| `records` / `items` / `count` | 标签 `1..15` 的事件列表 / 数量 |
| `date` | 事件日期 |
| `category_raw` / `category_name` | 事件标签 / 名称 |
| `c1_value` / `c2_value` / `c3_value` / `c4_value` | 按标签解释并完成单位换算的业务值 |
| `c1_float` / `c2_float` / `c3_float` / `c4_float` | 四字段的 float32 解释值 |
| `c1_raw` / `c2_raw` / `c3_raw` / `c4_raw` | 四个 wire 原始字段 |

标签 `1` 中四个业务值依次是每十股现金分红、配股价格、每十股送转数量、每十股配股数量。数量类标签的 `c*_value` 单位统一为股。

## AdjustmentFactorResponse / AdjustmentFactor

本地前、后复权仿射系数。

| 字段 | 含义 |
| --- | --- |
| `full_code` | 完整证券代码 |
| `anchor_date` | 用户指定的前复权事件截止自然日期；未指定时为 `None` |
| `start_date` | 用户指定的事件起点；未指定时为 `None` |
| `items` / `count` | 事件日期系数 / 数量 |
| `date` / `time` | 除权事件日期 / 当日 15:00 时间 |
| `qfq_scale` / `qfq_offset` | 前复权缩放和偏移 |
| `hfq_scale` / `hfq_offset` | 后复权缩放和偏移 |

价格应用公式为 `round(raw * scale + offset, 2)`。

## FinanceRecord

财务基础信息。

| 字段                              | 含义          |
| ------------------------------- | ----------- |
| `updated_date`                  | 财务数据更新日期    |
| `ipo_date`                      | 上市日期        |
| `circulating_shares`            | 流通股本，派生字段   |
| `total_shares`                  | 总股本，派生字段    |
| `total_assets_yuan`             | 总资产，派生字段    |
| `net_profit_yuan`               | 净利润，派生字段    |
| `eps_raw`                       | 每股收益原始值     |
| `province_raw` / `industry_raw` | 地区 / 行业原始编号 |

## SpecialLimitRecord

特殊品种涨跌停限制表。

| 字段                   | 含义     |
| -------------------- | ------ |
| `code` / `full_code` | 代码     |
| `limit_up_price`     | 涨停价    |
| `limit_down_price`   | 跌停价    |
| `record_hex`         | 单条记录原文 |

## WorkdayService

交易日工具。

| 方法                       | 含义               |
| ------------------------ | ---------------- |
| `refresh()`              | 用基准指数日 K 加载真实交易日 |
| `is_workday(date)`       | 判断是否交易日          |
| `previous_workday(date)` | 上一个交易日           |
| `next_workday(date)`     | 下一个交易日           |
| `range(start, end)`      | 交易日区间            |
| `today_is_workday()`     | 今天是否交易日          |

如果 `WorkdayService` 绑定了真实客户端，交易日来自基准指数日 K。对于超过当前 K 线范围的未来日期，`next_workday()` 可能返回 `None`。

## Helper 返回模型

### StockProfileTable / StockProfile

股票信息汇总。

| 字段                                                | 含义            |
| ------------------------------------------------- | ------------- |
| `codes`                                           | 请求代码          |
| `rows`                                            | 股票信息行         |
| `full_code` / `name`                              | 完整代码 / 名称     |
| `category` / `board`                              | 品种分类 / 板块分类   |
| `last_price` / `pre_close_price`                  | 最新价 / 昨收      |
| `change` / `change_pct`                           | 涨跌额 / 涨跌幅     |
| `volume_hand` / `amount`                          | 成交量，单位手 / 成交额 |
| `open_amount_yuan`                                | 开盘金额          |
| `circulating_shares` / `total_shares`             | 流通股本 / 总股本    |
| `turnover_rate`                                   | 本地计算换手率       |
| `circulating_market_value` / `total_market_value` | 流通市值 / 总市值    |
| `security` / `quote` / `finance`                  | 合并前的原始模型对象    |

### StockTopics / StockTopic

个股概念板块。

| 字段                             | 含义           |
| ------------------------------ | ------------ |
| `code`                         | 完整股票代码       |
| `topics`                       | 题材列表         |
| `topic_id` / `topic_name`      | 题材 ID / 题材名称 |
| `relation_level`               | 关联度          |
| `selected_date` / `topic_date` | 入选日期 / 题材日期  |
| `reason`                       | 入选原因         |
| `detail_id`                    | 详情记录 ID      |
| `source`                       | 合并来源         |
| `raw`                          | F10 原始行      |

### TopicStockTable / TopicStock

概念板块成分股。

| 字段                                  | 含义                 |
| ----------------------------------- | ------------------ |
| `seed_code`                         | 查询时使用的种子股票         |
| `topic_id` / `topic_name`           | 题材 ID / 题材名称       |
| `sort_by`                           | 排序字段               |
| `rows`                              | 成分股列表              |
| `rank`                              | 题材内排名              |
| `full_code` / `name`                | 完整代码 / 股票简称        |
| `change_pct`                        | 当日涨跌幅              |
| `change_pct_3d` / `change_pct_5d`   | 近 3 日 / 近 5 日涨跌幅   |
| `change_pct_20d` / `change_pct_60d` | 近 20 日 / 近 60 日涨跌幅 |
| `change_pct_ytd`                    | 年初以来涨跌幅            |
| `trading_date`                      | 统计日期               |
| `raw`                               | F10 原始行            |

### AuctionData

竞价组合结果。

| 字段                      | 含义           |
| ----------------------- | ------------ |
| `code` / `trading_date` | 完整代码 / 交易日   |
| `series`                | 当日或历史 `0x056a` 集合竞价过程快照 |
| `snapshot_0925`         | 09:25 正式撮合记录 |
| `pre_close_price`       | 前收盘参考价；普通日为昨收，除权除息日为调整后的除权参考价 |
| `open_price`            | 开盘价          |
| `open_volume`           | 09:25 成交量    |
| `open_amount`           | 09:25 成交额    |
| `open_change_pct`       | 开盘涨幅         |

### ShortlineIndicatorTable / ShortlineIndicator

短线指标 Helper。完整的统计、竞价、普通流通股本和封单字段定义见[短线指标](helpers/短线指标.md)。

| 字段 | 含义 |
| --- | --- |
| `codes` / `rows` / `count` | 请求代码 / 指标行 / 返回数量 |
| `target_trade_date` | 当前行情对应的目标交易日 |
| `previous_trade_date` | 上一实际交易日 |
| `stats_date` / `stats_source_path` | 统计文件日期 / 来源 |
| `stats_refreshed` | 本次是否重新下载统计文件 |
| `alignment_status` | 同日、上一交易日或个股行无法对齐状态 |
| `limit_status` | 当前封板、触板、未涨停或未知状态 |

下面是短线指标的统计字段和实时派生字段。字段名中的 `Z` 表示 TDX 自由流通口径，不是 Z-score；完整计算公式和日期对齐规则见[短线指标](helpers/短线指标.md)。

| 字段 | 中文名称 | 业务含义 | 单位 |
| --- | --- | --- | --- |
| `beta_60d` | 近 60 日 Beta | 近 60 日股价相对 TDX 资源所用市场基准的波动敏感度 | 无量纲 |
| `pe_ttm` | 滚动市盈率 | 当前估值相对最近 12 个月利润的倍数 | 倍 |
| `free_float_shares` | 流通股本Z | TDX 自由流通口径下可交易的股份数量 | 股 |
| `prev_amount` | 昨成交额 | 上一实际交易日全天成交金额 | 元 |
| `prev_seal_amount` | 昨封单额 | 上一实际交易日记录的涨停封单金额 | 元 |
| `prev2_seal_amount` | 前两日封单额 | 前第二个实际交易日记录的涨停封单金额 | 元 |
| `prev_open_volume_hand` | 昨开盘成交量 | 上一实际交易日 09:25 集合竞价成交量 | 手 |
| `prev_open_amount` | 昨开盘金额 | 上一实际交易日 09:25 集合竞价成交金额 | 元 |
| `limit_stat_days` | 涨停统计窗口 | “几天几板”使用的统计天数 | 天 |
| `limit_up_count_in_stat_days` | 统计期涨停次数 | 统计窗口内记录的涨停次数 | 次 |
| `limit_up_streak_days` | 文件连板天数 | 统计文件日期当时记录的连续涨停天数 | 天 |
| `year_limit_up_days` | 年内涨停天数 | 截至统计文件日期记录的当年累计涨停天数 | 天 |
| `free_float_market_value` | 流通市值Z | 自由流通股本按当前价计算的市值 | 元 |
| `open_turnover_z` | 开盘换手Z | 09:25 竞价成交量占自由流通股本的比例 | % |
| `open_prev_amount_ratio` | 开盘昨比 | 今日竞价成交额占昨日全天成交额的比例 | % |
| `auction_prev_volume_ratio` | 竞价昨比 | 今日竞价成交量相对昨日竞价成交量的倍数 | 倍 |
| `open_prev_seal_ratio` | 开盘昨封比 | 今日竞价成交额占昨日涨停封单额的比例 | % |
| `seal_to_float_ratio` | 封流比 | 当前买一金额占自由流通市值的比例；封单解读需确认当前已封板 | % |
| `seal_prev_ratio` | 封昨比（昨封比） | 当前买一金额相对昨日涨停封单额的倍数；封单解读需确认当前已封板 | 倍 |
| `limit_board_text` | 几天几板 | 统计窗口和窗口内涨停次数组成的文本 | 文本 |
| `ladder_level` | 当前连板高度 | 当前连续封住涨停的板数；未封板时为 `None` | 板 |
| `open_price` | 开盘价 | 当前交易日开盘价 | 元 |
| `pre_close` | 昨收价 | 当前交易日涨跌幅基准 | 元 |
| `open_change_pct` | 开盘涨幅 | 开盘价相对昨收的涨跌幅 | % |
| `open_amount` | 开盘金额 | 09:25 正式撮合金额 | 元 |
| `open_volume_hand` | 开盘成交量 | 09:25 正式撮合成交量 | 手 |
| `open_volume_ratio` | 开盘量比 | 相对近 5 个完整交易日平均每分钟成交量 | 倍 |
| `opening_rush` | 开盘抢筹 | 原生分类行情提供的开盘抢筹值 | % |
| `float_shares` | 普通流通股本 | 财务快照口径的流通股本 | 股 |
| `float_market_value` | 普通流通市值 | 普通流通股本对应的当前市值 | 元 |
| `seal_amount` | 当前买一金额 | 当前买一价乘买一量 | 元 |
| `seal_to_amount_ratio` | 封单成交额比 | 当前买一金额占当日成交额比例 | 倍 |

## 缓存口径

当前客户端只缓存部分 Helper 组合查询内部使用的财务批次、证券表和已验证的短线统计资源。

`client.codes.count()`、`client.codes.all()`、`client.corporate.capital_changes()`、`client.corporate.finance_batch()`、实时快照、分时、成交明细和 K 线均不缓存。

短线统计资源可用 `refresh_stats=True` 强制刷新；清空 Helper 缓存使用 `client.clear_cache()`。
