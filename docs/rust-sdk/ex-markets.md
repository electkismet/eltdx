---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 市场列表

## 作用

查询扩展行情服务器支持的市场。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient};

fn query(client: &ExClient) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.markets(&options)?;
    for market in &result {
        println!("{} {}", market.market().get(), market.name());
    }
    Ok(())
}
```

## 参数

无查询参数。`options` 是本次操作选项，示例使用默认值。

## 返回

返回市场列表。请求或参数错误通过 `Result` 返回。

从 `market().get()` 取得市场编号，从 `name()` 和 `short_name()` 取得显示名称。后续查商品时使用同一个服务器的市场编号。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/ex/struct.ExClient.html#method.markets)
