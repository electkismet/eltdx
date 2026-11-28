---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 个股特征快照

## 作用

读取证券特征快照。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例接收已配置的 MAC 客户端和证券身份，调用方使用完毕后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC 商品目录](mac-get-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, SymbolInfoQuery};

fn query(client: &MacClient, id: MacInstrumentId) -> Result<()> {
    let query = SymbolInfoQuery::new(id);
    let result = client.symbol_info(&query, &OperationOptions::default())?;
    println!("收盘价 {:?}", result.close());
    Ok(())
}
```

## 参数

传入 MAC 证券身份。`OperationOptions::default()` 使用默认操作选项。

## 返回

返回 `SymbolInfo`。请求及参数错误通过 `Result` 返回。

`pre_close()`、`open()`、`high()`、`low()`、`close()` 读取价格。`time()` 提供可解析的行情时间。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.symbol_info)
