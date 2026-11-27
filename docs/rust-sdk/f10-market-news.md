---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 首页要闻

## 作用

读取首页要闻。

## 示例

在 `Cargo.toml` 中启用 F10 功能。

```toml
eltdx = { version = "=3.4.0-beta.3", features = ["f10"] }
```

```rust
use eltdx::{OperationOptions, Result};
use eltdx::f10::{F10Client, MarketNewsQuery};

fn main() -> Result<()> {
    let query = MarketNewsQuery::new();
    let client = F10Client::builder().build()?;
    let options = OperationOptions::default();
    let result = client.market_news(&query, &options);
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

默认读取第 1 页，每页 50 条。`with_tab` 可选择首页栏目，`with_page` 和 `with_page_size` 可调整分页。

`options` 控制本次操作，示例使用默认值。

默认第 1 页，每页 50 条。`with_tab` 选择首页栏目，`with_page` 和 `with_page_size` 调整分页。

## 返回

返回 `Result<Response>`。请求错误通过 `Result` 返回。请求成功后还应检查 `Response::ok()`，它反映服务端的业务状态。

`ok()` 检查业务状态，`error_code()` 提供服务端错误码。`result_sets()` 遍历结果表，`columns()` 查看列名，`rows()` 和 `cells()` 读取记录与字段。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/f10/struct.F10Client.html#method.market_news)
