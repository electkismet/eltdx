---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 服务器交易时段

## 作用

读取服务器交易时段信息。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient};

fn query(client: &MacClient) -> Result<()> {
    let result = client.server_info(&OperationOptions::default())?;
    println!("最近交易日 {:?}", result.last_trading_day().parsed());
    Ok(())
}
```

## 参数

无需查询条件。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

## 返回

返回 `Result<ServerSession>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

用 `today()`、`last_trading_day()` 获取日期观测值，`sessions_1()` 和 `sessions_2()` 读取服务器提供的交易时段。日期可用 `parsed()` 检查能否转为交易日。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.server_info)
