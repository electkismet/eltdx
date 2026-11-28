---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 股本变迁

## 作用

批量读取证券股本变迁和权息事件。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{CapitalQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = CapitalQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.corporate().capital_changes(&[id], &query, &options);
    let closed = client.close();
    for item in result?.items() {
        println!("{} {:?}", item.security(), item.state());
    }
    closed
}
```

## 参数

`CapitalQuery::new()` 默认每批 75 个证券。`with_batch_size` 支持 1 到 200。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `BatchOutcome<CapitalChangeBlock>`，每个证券分别检查结果，成功的记录可通过 `records()` 读取。 网络、超时和参数错误通过 `Result` 返回。

- `CorporateService::capital_changes(&self, ids: &[SecurityId], query: &CapitalQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<CapitalChangeBlock>>`。

批量返回保留输入顺序。通过 `state()` 区分 `Present` 和 `ConfirmedMissing`，`input_index()` 可映射回原输入。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.CorporateService.html#method.capital_changes)
