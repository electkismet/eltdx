---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 历史开盘集合竞价

## 作用

读取指定交易日的开盘集合竞价记录。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{TradeQuery, MarketDateChoice, TradingDate};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = TradeQuery::new().with_date(MarketDateChoice::Date(TradingDate::new(2025, 1, 2)?))?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.trades().opening_match(&id, &query, &options);
    let closed = client.close();
    if let Some(tick) = result? {
        println!("{:?} 撮合价 {}", tick.time(), tick.price().as_f64());
    }
    closed
}
```

### 其他调用

`opening_match_many` 使用相同日期批量读取各证券的开盘撮合。每项成功结果仍可能为 `None`。在关闭客户端前调用。

```rust
let batch = client.trades().opening_match_many(&[id], &query, &options)?;
```

## 参数

`with_date` 指定要查询的交易日。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `Option<TradeTick>`，`None` 表示没有找到开盘集合竞价记录。 网络、超时和参数错误通过 `Result` 返回。

- `TradeService::opening_match(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)`，返回 `Result<Option<TradeTick>>`。
- `TradeService::opening_match_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<Option<TradeTick>>>`。

撮合记录的 `auction_matched()` 可读已撮合数量，未提供时为 `None`。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.TradeService.html#method.opening_match)
