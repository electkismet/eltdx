---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 集合竞价过程快照

## 作用

读取证券的集合竞价过程数据。`AuctionQuery` 可以选择查询日期。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
use eltdx::types::AuctionQuery;

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = AuctionQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.auctions().series(&id, &query, &options);
    let closed = client.close();
    let series = result?;
    println!("竞价点数 {}", series.points().len());
    closed
}
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `id: &SecurityId` | 要查询的证券。 |
| `query: &AuctionQuery` | 查询条件，默认查询当日。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

## 返回

`Result<AuctionSeries>` 包含竞价过程序列。序列中的竞价数据应结合其时间和状态使用。

通过 `points()` 取得竞价点。需要正式撮合时搭配 [集合竞价数据](auction-data.md)。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.AuctionService.html#method.series)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
