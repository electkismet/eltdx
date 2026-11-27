---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 单商品五档报价

## 作用

查询单个扩展市场商品报价。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 EX 商品目录](ex-info.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient, ExInstrumentId, QuoteQuery};

fn query(client: &ExClient, id: ExInstrumentId) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.quote(&QuoteQuery::new(id), &options)?;
    if let Some(quote) = result {
        println!("最新价 {:?}", quote.price());
    }
    Ok(())
}
```

## 参数

`id` 包含扩展市场 ID 和商品代码，应从扩展商品目录确认。`options` 是本次操作选项，示例使用默认值。

## 返回

返回 `Option<Quote>`，没有报价时为 `None`。请求或参数错误通过 `Result` 返回。

`None` 表示没有对应报价。`Some(quote)` 中，`price()` 是最新价，`pre_close()` 是前收，`bid()` 和 `ask()` 为五档买卖盘。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/ex/struct.ExClient.html#method.quote)
