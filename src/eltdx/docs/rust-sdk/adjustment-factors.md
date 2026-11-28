---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 复权因子

## 作用

根据股本变迁数据计算复权因子。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{FactorQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = FactorQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.corporate().adjustment_factors(&[id], &query, &options);
    let closed = client.close();
    for item in result?.items() {
        println!("{} {:?}", item.security(), item.state());
    }
    closed
}
```

## 参数

`FactorQuery::new()` 使用默认计算范围。`with_anchor_date`、`with_start_date` 指定可选日期，`with_capital` 调整股本查询。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `BatchOutcome<AdjustmentFactors>`。成功项的 `rows()` 提供日期以及前复权、后复权的比例和偏移。 网络、超时和参数错误通过 `Result` 返回。

- `CorporateService::adjustment_factors(&self, ids: &[SecurityId], query: &FactorQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<AdjustmentFactors>>`。

批量返回保留输入顺序。通过 `state()` 区分 `Present` 和 `ConfirmedMissing`，`input_index()` 可映射回原输入。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.CorporateService.html#method.adjustment_factors)
