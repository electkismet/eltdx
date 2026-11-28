---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 频道资讯

## 作用

按资讯频道读取新闻。

## 示例

在 `Cargo.toml` 中启用 F10 功能。

```toml
eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }
```

```rust
use eltdx::{OperationOptions, Result};
use eltdx::f10::{F10Client, ChannelNewsQuery, NewsChannel};

fn main() -> Result<()> {
    let query = ChannelNewsQuery::new(NewsChannel::FinanceHeadlines);
    let client = F10Client::builder().build()?;
    let options = OperationOptions::default();
    let result = client.channel_news(&query, &options);
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

构造时选择频道，默认第 1 页、每页 50 条。`with_page`、`with_page_size` 设置分页，`with_keyword` 设置关键词。

`options` 控制本次操作，示例使用默认值。

默认第 1 页，每页 50 条。用 `with_channel` 选择 [资讯频道](f10-news-channels.md)，`with_keyword` 设置关键字，`with_query1` 用于已知的频道附加参数。

## 返回

返回 `Result<Response>`。请求错误通过 `Result` 返回。请求成功后还应检查 `Response::ok()`，它反映服务端的业务状态。

`ok()` 检查业务状态，`error_code()` 提供服务端错误码。`result_sets()` 遍历结果表，`columns()` 查看列名，`rows()` 和 `cells()` 读取记录与字段。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/f10/struct.F10Client.html#method.channel_news)
