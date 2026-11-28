---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 市场盘口异动

## 作用

按市场读取盘口异动。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, UnusualQuery, MacMarketId};

fn query(client: &MacClient, market: MacMarketId) -> Result<()> {
    let query = UnusualQuery::new(market);
    let result = client.unusual(&query, &OperationOptions::default())?;
    for row in result.rows().iter().take(5) {
        println!("{:?} {}", row.time(), row.description());
    }
    Ok(())
}
```

## 参数

传入 MAC 市场标识，并可设置分页。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

`with_market` 指定市场，`with_start` 指定偏移，`with_count` 指定单页条数，`with_pages` 设置分页预算。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `Result<PageResult<Unusual>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

每条记录包含时间、异动类型、描述和值，通过 `time()`、`unusual_type()`、`description()` 和 `value()` 读取。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.unusual)
