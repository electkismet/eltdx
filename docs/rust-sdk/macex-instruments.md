---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 标的元数据

## 作用

分页读取扩展市场标的元数据。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::ex::ExMarketId;
use eltdx::mac::ex_query::InstrumentsQuery;

fn query(client: &MacExClient, market: ExMarketId) -> Result<()> {
    let query = InstrumentsQuery::new(market);
    let result = client.instruments(&query, &OperationOptions::default())?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

查询按扩展市场选择，默认从第 0 条开始。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

`InstrumentsQuery` 按扩展市场查目录，`with_start`、`with_count`、`with_pages` 控制分页。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<InstrumentInfo>`，检查行和分页状态。请求及参数错误通过 `Result` 返回。

通过 `rows()` 取本页商品，再通过每条记录的 `market()`、`code()`、`name()` 取得市场、代码和名称。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.instruments)
