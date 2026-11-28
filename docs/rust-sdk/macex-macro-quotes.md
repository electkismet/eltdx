---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 宏观指标最新值

## 作用

读取宏观指标最新报告值及前值等字段。

## 示例

在 `Cargo.toml` 中添加 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置并连接的客户端及目录中取得的指标代码。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::mac::ex_query::{MacroCode, MacroQuotesQuery};

fn query(client: &MacExClient, code_from_directory: &str) -> Result<()> {
    let code = MacroCode::new(code_from_directory)?;
    let query = MacroQuotesQuery::new(vec![code])?;
    let values = client.macro_quotes(&query, &OperationOptions::default())?;
    for row in &values {
        println!("{}，字段数 {}", row.name(), row.fields().len());
    }
    Ok(())
}
```

## 参数和使用

先用 `macro_indicators` 获取宏观指标目录，把其中的代码传给 `MacroCode::new`。`MacroQuotesQuery` 至少需要一个代码，`with_fields` 可调整返回字段。

## 返回

返回 `Vec<Quote>`，包含指标最新报告值及前值等字段。需要先配置并连接 MAC-EX 客户端。 网络和参数错误通过 `Result` 返回。



通过 `fields()` 查看返回字段。宏观指标应按指标自身的量纲解读，价格相关字段名沿用行情模型。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacExClient.html#method.macro_quotes)
