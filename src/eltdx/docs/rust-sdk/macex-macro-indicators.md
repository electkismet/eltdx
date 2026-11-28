---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 宏观指标列表

## 作用

读取宏观指标目录。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;

fn query(client: &MacExClient) -> Result<()> {
    let result = client.macro_indicators(&OperationOptions::default())?;
    for row in result.iter().take(5) {
        println!("{} {:?}", row.actual_code(), row.display_name());
    }
    Ok(())
}
```

## 参数

无需查询条件。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

## 返回

返回 `Vec<GoodsItem>`。请求及参数错误通过 `Result` 返回。

每条记录的 `actual_code()` 是后续查询使用的代码，`display_name()` 可能提供显示名称。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.macro_indicators)
