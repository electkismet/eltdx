---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 集合竞价数据

## 作用

组合集合竞价过程、正式撮合与前收盘参考价。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{AuctionDataQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = AuctionDataQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().auction_data(&id, &query, &options);
    let closed = client.close();
    let auction = result?;
    println!("交易日 {:?}，前收 {:?}", auction.date(), auction.pre_close());
    closed
}
```

## 参数

默认当前交易日，包含竞价过程、撮合和行情。`with_date` 指定日期，`with_series`、`with_snapshot`、`with_quote` 可选择数据，`with_pre_close` 可提供前收盘参考价。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

返回 `AuctionData`，包含所请求的竞价过程、撮合与参考价格，部分来源可能缺失。请求失败时通过 `Result` 返回错误。



用 `series()` 取得竞价过程。没有可用的行情来源或参考价格时，相应字段为 `None`。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.auction_data)
