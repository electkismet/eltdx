---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 宏观指标历史序列

## 作用

借用 K 线模型读取宏观指标的历史报告期序列。

## 示例

在 `Cargo.toml` 中添加 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例函数接收已配置并连接的客户端及目录中取得的指标代码。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::mac::ex_query::{MacroBarsQuery, MacroCode};

fn query(client: &MacExClient, code_from_directory: &str) -> Result<()> {
    let code = MacroCode::new(code_from_directory)?;
    let query = MacroBarsQuery::new(code)?;
    let values = client.macro_bars(&query, &OperationOptions::default())?;
    for bar in values.iter().take(5) {
        println!("{:?} {:?}", bar.time(), bar.close());
    }
    Ok(())
}
```

## 参数和使用

指标代码可从 `macro_indicators` 返回的目录取得。默认按月查询，可用 `with_period`、`with_start`、`with_count` 和 `with_pages` 调整周期与分页。

## 返回

返回 `Vec<Bar>`，每条代表一个历史报告期。需要先配置并连接 MAC-EX 客户端。 网络和参数错误通过 `Result` 返回。



通过 `time()` 查看时间，`close()` 等字段沿用 K 线模型。指标单位取决于对应的宏观指标。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.macro_bars)
