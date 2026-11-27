---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 特殊品种涨跌停限制

## 作用

读取主站提供的特殊品种涨跌停限制表。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, SpecialLimitQuery, TdxClient};

fn main() -> Result<()> {
    let query = SpecialLimitQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.limits().special(&query, &options);
    let closed = client.close();
    println!("{:?}", result?);
    closed
}
```

### 其他调用

`special` 读取一页记录，`scan_special` 按查询设置的预算连续读取。在关闭客户端前调用。

```rust
let records = client.limits().scan_special(&query, &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `query: &SpecialLimitQuery` | 查询条件，包括分页设置。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

## 返回

`special` 返回 `Result<DataPage<SpecialLimit>>`，表示一页记录。`scan_special` 返回 `Result<Collected<SpecialLimit>>`，会按查询条件连续扫描，应检查结果的完整性。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.LimitService.html#method.special)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
