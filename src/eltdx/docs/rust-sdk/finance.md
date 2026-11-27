---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 财务基础信息

## 作用

批量读取证券的基础财务数据，也可以指定需要的财务字段。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{FinanceQuery, OperationOptions, Result, SecurityId, TdxClient};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = FinanceQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.corporate().finance(&[id], &query, &options);
    let closed = client.close();
    for item in result?.items() {
        println!("{} {:?}", item.security(), item.state());
    }
    closed
}
```

### 其他调用

已知想读哪些字段时，传入 `FinanceField` 列表，例如总股本和每股收益。在关闭客户端前调用。

```rust
let fields = [eltdx::types::FinanceField::TotalShares, eltdx::types::FinanceField::Eps];
let batches = client.corporate().finance_fields(&[id], &fields, &query, &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `ids: &[SecurityId]` | 要查询的证券列表。 |
| `query: &FinanceQuery` | 财务查询条件。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |
| `fields: &[FinanceField]` | `finance_fields` 使用的字段列表。 |

## 返回

`finance` 返回 `Result<BatchOutcome<FinanceRecord>>`，`finance_fields` 返回 `Result<BatchOutcome<FinanceProjection>>`。批量结果保留各证券的独立状态，请逐项检查。

批量返回保留输入顺序。通过 `state()` 区分 `Present` 和 `ConfirmedMissing`，`input_index()` 可映射回原输入。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.CorporateService.html#method.finance)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
