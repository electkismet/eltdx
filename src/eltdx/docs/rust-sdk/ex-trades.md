---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 当日成交明细

## 作用

分页读取扩展商品当日成交明细。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 EX 商品目录](ex-info.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient, ExInstrumentId, TransactionsQuery};

fn query(client: &ExClient, id: ExInstrumentId) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.transactions(&TransactionsQuery::new(id), &options)?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

`with_start` 和 `with_count` 设置起始位置和数量。`options` 是本次操作选项，示例使用默认值。

`with_start` 和 `with_count` 分别设置起点和本次读取条数。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<Transaction>`，检查行和分页状态。请求或参数错误通过 `Result` 返回。

逐条读取 `time()`、`price_raw()`、`volume()`。`price_raw()` 与 `direction_raw()` 保留来源值，解释时需结合商品的报价精度和市场含义。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/ex/struct.ExClient.html#method.transactions)
