---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 单日分时

## 作用

读取证券单日分时。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例接收已配置的 MAC 客户端和证券身份，调用方使用完毕后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC 商品目录](mac-get-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, TickChartQuery};

fn query(client: &MacClient, id: MacInstrumentId) -> Result<()> {
    let query = TickChartQuery::new(id);
    let result = client.tick_chart(&query, &OperationOptions::default())?;
    println!("分时点数 {}", result.ticks().len());
    Ok(())
}
```

## 参数

`with_date` 可指定交易日期。`OperationOptions::default()` 使用默认操作选项。

`TickChartQuery` 接收证券身份。`with_date(Some(date))` 指定交易日，默认查询当前分时。

## 返回

返回 `TickChart`。请求及参数错误通过 `Result` 返回。

通过 `ticks()` 读取分时点，`date_raw()` 是服务器给出的原始日期。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.tick_chart)
