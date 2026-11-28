---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 批量报价

## 作用

读取多个证券的报价。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例接收已配置的 MAC 客户端和证券身份，调用方使用完毕后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC 商品目录](mac-get-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, QuotesQuery};

fn query(client: &MacClient, id: MacInstrumentId) -> Result<()> {
    let query = QuotesQuery::new(vec![id])?;
    let result = client.quotes(&query, &OperationOptions::default())?;
    for row in result.iter().take(5) {
        println!("{}，字段数 {}", row.name(), row.fields().len());
    }
    Ok(())
}
```

## 参数

可传入多个证券身份。`OperationOptions::default()` 使用默认操作选项。

`QuotesQuery::new` 接受标的列表。`with_fields` 设置字段集合，只需要部分行情时可选择对应字段。

## 返回

返回 `Vec<Quote>`，每个报价保留所请求字段及来源。请求及参数错误通过 `Result` 返回。

通过 `id()`、`name()` 区分证券。`fields()` 列出请求返回的字段，可用 `field(FieldId)` 查单项。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.quotes)
