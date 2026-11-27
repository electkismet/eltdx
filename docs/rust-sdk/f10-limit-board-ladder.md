---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 涨跌停列表

## 作用

读取服务端提供的涨跌停列表。

## 示例

在 `Cargo.toml` 中启用 F10 功能。

```toml
eltdx = { version = "=3.4.0-beta.3", features = ["f10"] }
```

```rust
use eltdx::{OperationOptions, Result};
use eltdx::f10::{F10Client, LimitUpDownListQuery};

fn main() -> Result<()> {
    let query = LimitUpDownListQuery::new();
    let client = F10Client::builder().build()?;
    let options = OperationOptions::default();
    let result = client.limit_up_down_list(&query, &options);
    let closed = client.close();
    let response = result?;
    if response.ok() {
        println!("涨跌停记录 {} 条，市场概况 {} 条", response.rows().len(), response.summary().len());
    } else {
        eprintln!("服务端错误码 {:?}", response.error_code());
    }
    closed
}
```

## 参数

默认日期范围由客户端解析。可用 `with_dates` 指定起止日期，用 `with_include_summary(true)` 请求摘要。

`options` 控制本次操作，示例使用默认值。

通过 `with_start_date`、`with_end_date` 或 `with_dates` 选择日期范围，`with_include_summary` 控制是否请求汇总。

## 返回

返回 `Result<LimitBoardLadder>`。请求错误通过 `Result` 返回。`rows()` 读取个股涨跌停记录，`summary()` 读取市场概况，`ladder_counts_by_date()` 读取按日期汇总的连板数量。分别用 `ok()` 和 `summary_ok()` 检查明细和汇总的业务状态。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/f10/struct.F10Client.html#method.limit_up_down_list)
