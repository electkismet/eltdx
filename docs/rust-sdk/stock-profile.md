---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 股票信息汇总

## 作用

合并行情快照、代码表和财务基础信息，形成股票表头信息。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{ProfileQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = ProfileQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().stock_profile_table(&[id], &query, &options);
    let closed = client.close();
    for row in result?.rows().iter().take(5) {
        println!("{} {:?} 最新价 {:?}", row.id(), row.name(), row.last());
    }
    closed
}
```

## 参数

示例查询一只股票，数组可包含多只证券。默认包含代码表和财务信息，`with_security(false)?`、`with_finance(false)?` 可分别关闭。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

每行可用 `id()`、`name()`、`last()` 查看证券、名称和最新价。财务或证券资料可能缺失。请求失败时通过 `Result` 返回错误。



行情是必需来源。代码表和财务为独立补充，缺失时可检查 `security_status()` 和 `finance_status()`。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.stock_profile_table)
