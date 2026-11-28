---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 分类报价列表

## 作用

分页查询指定市场和分类的报价。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient, ExMarketId, QuoteCategory, QuoteListQuery};

fn query(client: &ExClient, market: ExMarketId) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.quote_list(&QuoteListQuery::new(market, QuoteCategory::HongKong), &options)?;
    for row in result.rows().iter().take(5) {
        println!("{:?}，字段数 {}", row.id(), row.fields().len());
    }
    Ok(())
}
```

## 参数

示例选择港股分类，`QuoteCategory::Futures` 选择期货分类。`with_start` 和 `with_count` 设置分页。`options` 是本次操作选项，示例使用默认值。

`QuoteListQuery` 接收市场与报价类别，两者需与目标商品一致。用 `with_start`、`with_count` 设置偏移和页长。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<QuoteListRow>`，检查行和分页状态。请求或参数错误通过 `Result` 返回。

每行通过 `id()` 识别商品，`fields()` 列出来源字段。不同的报价分类可能提供不同字段。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/ex/struct.ExClient.html#method.quote_list)
