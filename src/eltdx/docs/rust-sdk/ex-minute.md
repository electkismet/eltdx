---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 当日分时

## 作用

读取扩展商品当日分时。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 EX 商品目录](ex-info.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient, ExInstrumentId, MinutesQuery};

fn query(client: &ExClient, id: ExInstrumentId) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.minutes(&MinutesQuery::new(id), &options)?;
    println!("返回 {} 条", result.len());
    Ok(())
}
```

## 参数

`id` 是扩展市场商品身份。`options` 是本次操作选项，示例使用默认值。

## 返回

返回 `Vec<Minute>`。请求或参数错误通过 `Result` 返回。

每条记录的 `time()`、`price()`、`avg_price()`、`volume()` 分别提供时间、价格、均价和成交量。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/ex/struct.ExClient.html#method.minutes)
