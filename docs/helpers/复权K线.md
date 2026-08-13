---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# 复权 K 线

## 作用

用更顺手的参数获取不复权、前复权、后复权和定点复权 K 线。

| 项目 | 内容 |
| --- | --- |
| 调用方法 | `client.helpers.adjusted_kline(code, ...)` |
| 返回模型 | `KlineSeries` |
| 底层能力 | [`0x052d` K 线周期线](../methods/7709-K线周期线.md)、[`全量 K 线分页`](../methods/7709-全量K线分页.md) |

## 示例

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    qfq = client.helpers.adjusted_kline("sz000001", period="day", adjust="qfq", count=200)
    hfq = client.helpers.adjusted_kline("sz000001", period="week", adjust="hfq", count=100)
    fixed = client.helpers.adjusted_kline(
        "sz000001",
        period="day",
        adjust="fixed_qfq",
        anchor_date="2024-06-03",
        count=200,
    )

print(qfq.adjust_mode, qfq.bars[-1].close)
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `code` | 股票代码 |
| `period` | 周期，默认 `day` |
| `adjust` | 复权模式 |
| `anchor_date` | 定点复权日期 |
| `count` / `start` | 单页查询时的数量和起点 |
| `all_pages` | 是否自动分页拉全 |
| `page_size` / `max_pages` | 自动分页参数 |
| `include_raw` | 是否保留原始 payload |

## 周期

| `period` | 含义 |
| --- | --- |
| `1m`, `5m`, `15m`, `30m`, `60m` | 分钟 K 线 |
| `day`, `week`, `month`, `quarter`, `year` | 日、周、月、季、年 |

## 复权

| `adjust` | 含义 |
| --- | --- |
| `None` / `none` | 不复权 |
| `qfq` / `front` | 前复权 |
| `hfq` / `back` | 后复权 |
| `fixed_qfq` / `fixed_front` | 定点前复权 |
| `fixed_hfq` / `fixed_back` | 定点后复权 |

## 拉全量

```python
series = client.helpers.adjusted_kline(
    "sz000001",
    period="day",
    adjust="qfq",
    all_pages=True,
    page_size=800,
)
```

`all_pages=True` 时，Helper 会调用 `client.bars.all()` 自动分页合并。
