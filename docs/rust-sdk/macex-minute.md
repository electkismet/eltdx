---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 单日分时

## 作用

读取扩展标的单日分时。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC-EX 商品目录](macex-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::ex::ExInstrumentId;
use eltdx::mac::ex_query::TickChartQuery;

fn query(client: &MacExClient, id: ExInstrumentId) -> Result<()> {
    let query = TickChartQuery::new(id)?;
    let result = client.tick_chart(&query, &OperationOptions::default())?;
    println!("分时点数 {}", result.ticks().len());
    Ok(())
}
```

## 参数

可在查询条件中设置日期。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

`with_date(Some(date))` 可指定交易日。省略日期时使用默认日期。

## 返回

返回 `TickChart`。请求及参数错误通过 `Result` 返回。

`ticks()` 给出分时点，`date_raw()` 保留服务器返回的日期值。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.tick_chart)
