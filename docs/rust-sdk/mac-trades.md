---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 成交明细

## 作用

分页读取证券成交明细。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例接收已配置的 MAC 客户端和证券身份，调用方使用完毕后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC 商品目录](mac-get-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, TransactionsQuery};

fn query(client: &MacClient, id: MacInstrumentId) -> Result<()> {
    let query = TransactionsQuery::new(id);
    let result = client.transactions(&query, &OperationOptions::default())?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

可在查询条件中设置分页和日期。`OperationOptions::default()` 使用默认操作选项。

`with_date` 选择可选交易日，`with_start`、`with_count`、`with_pages` 设置起始位置和分页限制。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<Transaction>`，检查行和分页状态。请求及参数错误通过 `Result` 返回。

每页记录包含成交时间、方向及价格等。查看 `completion()` 和 `next_offset()` 判断是否还要继续读。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.transactions)
