---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 旧版批量报价

## 作用

按证券列表读取旧版报价，每个证券保留独立结果。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.quotes().legacy(&[id], &options);
    let closed = client.close();
    for item in result?.items() {
        println!("{} {:?}", item.security(), item.state());
    }
    closed
}
```

## 参数

证券列表传入切片，可放入多个 `SecurityId`。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

结果为 `BatchOutcome<LegacyQuote>`，逐项检查成功或失败。 网络、超时和参数错误通过 `Result` 返回。

- `QuoteService::legacy(&self, ids: &[SecurityId], options: &OperationOptions)`，返回 `Result<BatchOutcome<LegacyQuote>>`。

批量返回保留输入顺序。通过 `state()` 区分 `Present` 和 `ConfirmedMissing`，`input_index()` 可映射回原输入。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.QuoteService.html#method.legacy)
