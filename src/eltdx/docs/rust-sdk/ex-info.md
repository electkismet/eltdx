---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 商品信息

## 作用

分页查询扩展行情商品目录。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient, InstrumentsQuery};

fn query(client: &ExClient) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.instruments(&InstrumentsQuery::new(0).with_count(10)?, &options)?;
    for item in result.rows().iter().take(5) {
        println!("{} {} {}", item.market().get(), item.code(), item.name());
    }
    Ok(())
}
```

## 参数

`InstrumentsQuery::new(0)` 从第 0 条开始，`with_count` 设置每页数量。`options` 是本次操作选项，示例使用默认值。

`InstrumentsQuery::new(start)` 指定目录的起始位置，从 0 开始，`with_count` 调整本页条数。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<InstrumentInfo>`，可通过 `rows()`、`completion()` 和 `next_offset()` 检查数据和分页状态。请求或参数错误通过 `Result` 返回。

通过 `rows()` 读取本页，每条记录的 `market()`、`code()` 和 `name()` 可用于选取后续查询的证券。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/ex/struct.ExClient.html#method.instruments)
