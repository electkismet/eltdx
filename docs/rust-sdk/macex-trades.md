---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 成交明细

## 作用

分页读取扩展标的成交明细。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC-EX 商品目录](macex-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::ex::ExInstrumentId;
use eltdx::mac::ex_query::TransactionsQuery;

fn query(client: &MacExClient, id: ExInstrumentId) -> Result<()> {
    let query = TransactionsQuery::new(id)?;
    let result = client.transactions(&query, &OperationOptions::default())?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

可在查询条件中设置日期和分页。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

用 `with_date` 设置可选交易日，`with_start`、`with_count` 设置分页，`with_pages` 控制预算。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<Transaction>`，检查行和分页状态。请求及参数错误通过 `Result` 返回。

`rows()` 提供本页成交，`completion()` 和 `next_offset()` 说明本次分页的进度。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.transactions)
