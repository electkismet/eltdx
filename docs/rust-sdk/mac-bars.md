---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 复权 K 线

## 作用

读取证券 K 线。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例接收已配置的 MAC 客户端和证券身份，调用方使用完毕后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC 商品目录](mac-get-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, BarsQuery};

fn query(client: &MacClient, id: MacInstrumentId) -> Result<()> {
    let query = BarsQuery::new(id);
    let result = client.bars(&query, &OperationOptions::default())?;
    for bar in result.iter().take(5) {
        println!("{:?} {:?}", bar.time(), bar.close());
    }
    Ok(())
}
```

## 参数

可通过查询条件设置周期、复权及分页。`OperationOptions::default()` 使用默认操作选项。

`with_period` 设置周期，`with_times` 设置周期倍数，`with_start`、`with_count`、`with_pages` 调整查询范围。`with_adjust` 选择复权方式，`with_bar_time` 设置时间标签。

## 返回

返回 `Vec<Bar>`。请求及参数错误通过 `Result` 返回。

通过 `time()` 和 `open()`、`high()`、`low()`、`close()` 读取时间与价格。需要区分原始和调整后的价格时，检查返回价格对象的来源。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.bars)
