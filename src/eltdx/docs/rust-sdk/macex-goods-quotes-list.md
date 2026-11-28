---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 列表报价

## 作用

读取指定扩展市场的报价列表。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::ex::ExMarketId;
use eltdx::mac::ex_query::QuoteListQuery;

fn query(client: &MacExClient, market: ExMarketId) -> Result<()> {
    let query = QuoteListQuery::new(market);
    let result = client.quote_list(&query, &OperationOptions::default())?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

可在查询条件中设置排序与分页。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

`with_order` 可以设置榜单顺序，`with_start`、`with_count`、`with_pages` 调整分页。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<Quote>`，检查行和分页状态。请求及参数错误通过 `Result` 返回。

用 `rows()` 遍历本页，逐条读取 `id()`、`name()` 和 `fields()`。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacExClient.html#method.quote_list)
