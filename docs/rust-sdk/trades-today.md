---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 当日成交明细

## 作用

读取当前交易日的逐笔成交明细。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{TradeQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = TradeQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.trades().page(&id, &query, &options);
    let closed = client.close();
    let page = result?;
    for tick in page.ticks().items().iter().take(5) {
        println!("{:?} 成交价 {}", tick.time(), tick.price().as_f64());
    }
    closed
}
```

### 其他调用

`page` 和 `collect` 查一只证券，分别取单页和预算内的全部页。`page_many`、`collect_many` 以相同的日期和分页设置查询多只证券。在关闭客户端前调用。

```rust
let service = client.trades();
let trades = service.collect(&id, &query, &options)?;
let pages = service.page_many(&[id.clone()], &query, &options)?;
let histories = service.collect_many(&[id.clone()], &query, &options)?;
```

## 参数

`TradeQuery::new()` 默认当前交易日、起点 0、每页 1800 条。`with_start`、`with_count` 和 `with_pages` 可设置分页，单页数量为 1 到 1800。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `TradePage`。自动翻页受 `with_pages` 设置的预算限制。 网络、超时和参数错误通过 `Result` 返回。

- `TradeService::page(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)`，返回 `Result<TradePage>`。
- `TradeService::collect(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions)`，返回 `Result<TradePage>`。
- `TradeService::page_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<TradePage>>`。
- `TradeService::collect_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<TradePage>>`。

`ticks().items()` 读取明细。`time()`、`price()`、`volume()` 和 `side()` 分别提供时间、价格、数量和方向。用 `ticks().end()` 检查结束状态。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.TradeService.html#method.page)
