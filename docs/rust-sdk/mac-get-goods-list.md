---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 商品列表

## 作用

分页读取指定市场商品列表。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, GoodsQuery, MacMarketId};

fn query(client: &MacClient, market: MacMarketId) -> Result<()> {
    let query = GoodsQuery::new(market);
    let result = client.goods(&query, &OperationOptions::default())?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

传入市场标识，可设置分页。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

`GoodsQuery` 接收市场编号。`with_start`、`with_count`、`with_pages` 调整起始位置、数量和分页预算。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `Result<PageResult<GoodsItem>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

遍历 `rows()`，通过 `actual_code()` 与 `display_name()` 读取代码和可能存在的显示名称。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.goods)
