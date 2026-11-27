---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 多日分时

## 作用

读取证券多日分时。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例接收已配置的 MAC 客户端和证券身份，调用方使用完毕后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC 商品目录](mac-get-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, TickChartsQuery};

fn query(client: &MacClient, id: MacInstrumentId) -> Result<()> {
    let query = TickChartsQuery::new(id);
    let result = client.tick_charts(&query, &OperationOptions::default())?;
    for day in result.days() {
        println!("{:?} 共 {} 条", day.date(), day.ticks().len());
    }
    Ok(())
}
```

## 参数

默认查询 5 日，`with_days` 支持 1 到 5 日，`with_date` 可指定日期。`OperationOptions::default()` 使用默认操作选项。

`with_days` 设置天数，`with_date` 设置可选交易日。返回中的实际日期以 `days()` 给出的日期为准。

## 返回

返回 `MultiTickChart`。请求及参数错误通过 `Result` 返回。

通过 `days()` 遍历日期，每天的 `date()` 与 `ticks()` 分别给出交易日和该日分时点。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.tick_charts)
