---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 退市证券日线

## 作用

读取退市证券的不复权日线。示例证券代码需替换为实际退市证券。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{DelistedQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = DelistedQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.klines().delisted_page(&id, &query, &options);
    let closed = client.close();
    let series = result?;
    for bar in series.bars().items().iter().take(5) {
        println!("收盘价 {}", bar.close().as_f64());
    }
    closed
}
```

### 其他调用

`delisted_page` 和 `delisted_collect` 查一只退市证券，分别读取一页和多页。`delisted_page_many`、`delisted_collect_many` 批量查询多只退市证券。在关闭客户端前调用。

```rust
let service = client.klines();
let history = service.delisted_collect(&id, &query, &options)?;
let pages = service.delisted_page_many(&[id.clone()], &query, &options)?;
let histories = service.delisted_collect_many(&[id.clone()], &query, &options)?;
```

## 参数

`DelistedQuery::new()` 默认从第 0 条读取，每页最多 800 条。使用 `with_start`、`with_count` 或 `with_pages` 设置分页。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `KlineSeries`。自动翻页方法受查询中的分页预算限制。 网络、超时和参数错误通过 `Result` 返回。

- `KlineService::delisted_page(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions)`，返回 `Result<KlineSeries>`。
- `KlineService::delisted_collect(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions)`，返回 `Result<KlineSeries>`。
- `KlineService::delisted_page_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<KlineSeries>>`。
- `KlineService::delisted_collect_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<KlineSeries>>`。

用 `bars().items()` 读取日线记录。每条包含开高低收和成交量额。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.KlineService.html#method.delisted_page)
