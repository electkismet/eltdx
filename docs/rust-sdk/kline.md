---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# K 线

## 作用

读取指定证券的 K 线，可以选择周期、复权方式和分页条件。`page` 读取一页，`collect` 自动翻页，带 `_many` 的方法用于批量查询。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
use eltdx::types::{AdjustMode, KlinePeriod, KlineQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = KlineQuery::new()
        .with_period(KlinePeriod::Day)?
        .with_adjust(AdjustMode::None)?
        .with_count(10)?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.klines().page(&id, &query, &options);
    let closed = client.close();
    let series = result?;
    for bar in series.bars().items().iter().take(5) {
        println!("收盘价 {}", bar.close().as_f64());
    }
    closed
}
```

### 其他调用

`page` 读一只证券的一页，`collect` 读一只证券的多页。`page_many` 和 `collect_many` 用相同查询条件处理多个证券。通过 `with_pages` 限制自动翻页，通过 `with_cursor` 接续此前的一页。在关闭客户端前调用。

```rust
let service = client.klines();
let history = service.collect(&id, &query, &options)?;
let pages = service.page_many(&[id.clone()], &query, &options)?;
let histories = service.collect_many(&[id.clone()], &query, &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `id: &SecurityId` | 单个证券身份，例如上海的 `sh600000`。 |
| `ids: &[SecurityId]` | 批量方法使用的证券身份列表。 |
| `query: &KlineQuery` | 查询条件。示例选择日线、不复权和每页 10 条。`with_count` 的范围是 1 到 800。 |
| `options: &OperationOptions` | 本次操作的选项。 |

## 返回

单证券方法返回 `Result<KlineSeries>`。批量方法返回 `Result<BatchOutcome<KlineSeries>>`，应逐项检查状态。自动翻页受查询中的分页预算限制，读取后应检查结果的完整性信息。

`bars().items()` 是 K 线记录。每根 K 线有 `open()`、`high()`、`low()`、`close()`、`volume()` 和 `amount()`。用 `bars().end()` 检查数据结束情况。

[查看 K 线方法和类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.KlineService.html)

## 出错与关闭

网络、超时或参数错误会通过 `Result` 返回。示例先保存查询结果并关闭客户端，再用 `?` 检查查询结果。
