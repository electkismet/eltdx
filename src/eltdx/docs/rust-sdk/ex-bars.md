---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 周期 K 线

## 作用

查询扩展商品指定周期的 K 线。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 EX 商品目录](ex-info.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient, BarsQuery, ExInstrumentId, ExPeriod};

fn query(client: &ExClient, id: ExInstrumentId) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.bars(&BarsQuery::new(id, ExPeriod::Day), &options)?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

示例使用日线。`ExPeriod` 还提供分钟、周、月等周期，`with_start` 和 `with_count` 设置分页。`options` 是本次操作选项，示例使用默认值。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<Bar>`，检查行和分页状态。请求或参数错误通过 `Result` 返回。

`time()` 给出 K 线时间，`open()`、`high()`、`low()`、`close()` 给出四个价格，`time_source()` 可区分打包分钟和补齐的收盘时间。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/ex/struct.ExClient.html#method.bars)
