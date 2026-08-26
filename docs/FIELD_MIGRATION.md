# 历史字段对照

这份文档给从历史版本迁移到 `eltdx 2.0.5` 的用户看。`2.0` 已移除 `TdxClient` 上旧版扁平 `get_*` 入口；完整方法迁移表见 [迁移到 eltdx 2.0](MIGRATION_FROM_OLD.md)。

## raw 调试字段

旧版常见写法：

```python
data = client.get_kline("day", "sz000001", include_raw=True)
print(data.raw_frame_hex)
print(data.raw_payload_hex)
```

`2.0.1` 模型主要保留 payload 和单条记录原文：

```python
data = client.bars.get("sz000001", period="day", include_raw=True)
print(data.raw_payload.hex())
print(data.bars[0].record_hex)
```

对照：

| 旧字段 | 新字段 / 新写法 | 说明 |
| --- | --- | --- |
| `raw_payload_hex` | `raw_payload.hex()` | payload 原始十六进制 |
| `items[].raw_hex` | `bars[].record_hex` / `points[].record_hex` / `ticks[].record_hex` / `records[].record_hex` | 单条记录原始十六进制 |
| `raw_frame_hex` | 查看 transport 返回帧或抓包样本 | 业务模型默认保留 payload；完整 TCP 响应帧更适合在 transport / 抓包层查看 |

常规字段排查优先看 `raw_payload` 和单条 `record_hex`；需要完整帧时，在 transport / 抓包层取更清楚。

## 返回集合字段

旧版很多响应统一叫 `items`。新版按业务换成更直观的名字。

| 旧字段 | 新字段 |
| --- | --- |
| K 线 `items` | `bars` |
| 分时 `items` | `points` |
| 成交明细 `items` | `ticks` |
| 股本变迁 `items` | `records`，同时保留 `items` 属性 |
| 分类行情 `items` | `records` |

## 价格和涨跌幅

| 旧字段习惯 | 新字段 / 新写法 |
| --- | --- |
| `last_price` | `last_price` |
| `last_close_price` | `pre_close_price` |
| `change_percent` | `change_pct` |
| `amount` | `amount` |
| `volume` | 快照用 `total_hand`，成交明细用 `volume`，K 线用 `volume_lots` |

## 代码字段

| 旧字段习惯 | 新字段 / 新写法 |
| --- | --- |
| `code` 带市场前缀 | `full_code` |
| 市场 | `exchange` |
| 六位代码 | `code` |

`2.0.1` 推荐内部拆开保存：

```python
item.exchange   # "sz"
item.code       # "000001"
item.full_code  # "sz000001"
```

## 复权

旧版普通复权更多依赖本地因子计算。`2.0.1` 优先使用 `0x052d` 服务端复权参数：

```python
client.bars.get("sz000001", period="day", adjust="qfq")
client.bars.get("sz000001", period="day", adjust="hfq")
```

需要本地审计时使用完整仿射系数：

```python
factors = client.corporate.adjustment_factors("sz000001")
```

该接口只从 `0x000f` 生成事件级系数，不直接返回复权 K 线。应按 K 线日期选择系数行后计算 `round(raw * scale + offset, 2)`；选行代码见 [本地复权系数](methods/7709-本地复权系数.md)。

## 缓存

`2.0.1` 只缓存部分 Helper 组合查询使用的低频数据：

| 数据 | 默认缓存 |
| --- | --- |
| `client.corporate.capital_changes()` 股本变迁结果 | 否 |
| `client.helpers.stock_profile_table()` 内部财务批次 | 是 |
| 已验证的短线统计资源 | 是 |
| 代码数量、全量代码表、直接财务查询 | 否 |
| 行情快照、分时、成交明细、K 线 | 否 |

短线统计资源可强制重新请求：

```python
client.helpers.shortline_indicators("sz000001", refresh_stats=True)
```

需要清空全部缓存：

```python
client.clear_cache()
```
