---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC K 线索引偏移

## 作用

读取服务器提供的 K 线索引偏移。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, KlineOffsetsQuery};

fn query(client: &MacClient) -> Result<()> {
    let query = KlineOffsetsQuery::new();
    let result = client.kline_offsets(&query, &OperationOptions::default())?;
    println!("共 {} 条，本次返回 {} 条", result.total(), result.returned());
    Ok(())
}
```

## 参数

通过查询条件选择请求范围。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

## 返回

返回 `Result<KlineOffsetInfo>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

`total()` 是服务端报告数量，`returned()` 是响应中的返回数量。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.kline_offsets)
