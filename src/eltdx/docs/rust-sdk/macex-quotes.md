---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 批量报价与净值

## 作用

批量读取扩展标的报价。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC-EX 商品目录](macex-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::ex::ExInstrumentId;
use eltdx::mac::ex_query::QuotesQuery;

fn query(client: &MacExClient, id: ExInstrumentId) -> Result<()> {
    let query = QuotesQuery::new(vec![id])?;
    let result = client.quotes(&query, &OperationOptions::default())?;
    for row in result.iter().take(5) {
        println!("{}，字段数 {}", row.name(), row.fields().len());
    }
    Ok(())
}
```

## 参数

可传入多个扩展标的身份。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

`QuotesQuery` 接受证券身份列表，`with_fields` 可选择具体报价字段。

## 返回

返回 `Vec<Quote>`。请求及参数错误通过 `Result` 返回。

每条报价保存 `id()`、`name()` 及所请求的 `fields()`。基金净值等字段取决于市场和字段选择。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.quotes)
