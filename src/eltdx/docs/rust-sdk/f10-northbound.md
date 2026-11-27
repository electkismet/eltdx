---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 沪深股通持仓

## 作用

返回沪深股通持股比例、持股数量、变动股数和收盘价等序列。

## 示例

在 `Cargo.toml` 中启用 F10 功能。

```toml
eltdx = { version = "=3.4.0-beta.3", features = ["f10"] }
```

```rust
use eltdx::{OperationOptions, Result};
use eltdx::f10::{AshareCode, F10Client, F10MarketCode, NorthboundHoldingQuery};

fn main() -> Result<()> {
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    let query = NorthboundHoldingQuery::new(code);
    let client = F10Client::builder().build()?;
    let options = OperationOptions::default();
    let result = client.northbound_holding(&query, &options);
    let closed = client.close();
    let response = result?;
    if !response.ok() {
        eprintln!("服务端错误码：{:?}", response.error_code());
    } else if let Some(table) = response.first_table() {
        println!("列：{:?}", table.columns());
        for row in table.rows().iter().take(3) {
            for cell in row.cells() {
                println!("{} = {:?}", cell.name(), cell.value());
            }
        }
    }
    closed
}
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `query: &NorthboundHoldingQuery` | 查询条件。示例使用上海股票 `600000`，代码和市场通过 `AshareCode` 指定。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

用 `with_section` 选择栏目，`with_filter_value` 设置栏目所需的筛选值，`with_page`、`with_page_size` 调整分页。

## 返回

- `northbound_holding` 返回 `Result<Response>`。

先用 `ok()` 检查服务端业务状态。服务端返回错误码时通过 `error_code()` 查看。`result_sets()` 可以遍历全部结果表，`first_table()` 取第一张表。表内的 `columns()`、`rows()` 分别提供列名和记录，`cells()` 逐个读取字段并保留重复列名。不同栏目可能返回不同的列或空表。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/f10/struct.F10Client.html#method.northbound_holding)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
