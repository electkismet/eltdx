---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 个股资金流

## 作用

读取证券资金流数据。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例接收已配置的 MAC 客户端和证券身份，调用方使用完毕后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC 商品目录](mac-get-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, CapitalFlowQuery};

fn query(client: &MacClient, id: MacInstrumentId) -> Result<()> {
    let query = CapitalFlowQuery::new(id)?;
    let result = client.capital_flow(&query, &OperationOptions::default())?;
    if let Some(flow) = result {
        println!("主力净流入 {:?}", flow.main_net().value());
    }
    Ok(())
}
```

## 参数

传入 MAC 证券身份，代码编码后长度最多 8 字节。`OperationOptions::default()` 使用默认操作选项。

## 返回

返回 `Option<CapitalFlow>`，没有数据时为 `None`。请求及参数错误通过 `Result` 返回。

`None` 表示没有对应记录。存在记录时，`main_in()`、`main_out()`、`main_net()` 可读主力流入、流出、净流入。用数值对象的 `value()` 检查是否有数字。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.capital_flow)
