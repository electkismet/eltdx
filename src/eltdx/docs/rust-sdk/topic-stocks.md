---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 概念板块成分股

## 作用

按题材 ID 或名称查询概念板块成分股，并整理排名和区间涨跌幅。

## 示例

在 `Cargo.toml` 中添加 `eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::f10::{AshareCode, F10MarketCode};
use eltdx::helpers::TopicStocksQuery;

fn main() -> Result<()> {
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    let query = TopicStocksQuery::first(code);
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().topic_stocks(&query, &options);
    let closed = client.close();
    let table = result?;
    for stock in table.rows().iter().take(5) {
        println!("排名 {:?}，名称 {:?}", stock.rank(), stock.name());
    }
    closed
}
```

## 参数

需启用 `f10`。`first(code)` 选择该股票发现的第一个题材。已知题材 ID 时使用 `new(code, id)?`，按名称选择时使用 `named(code, name)?`。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

结果整理题材成分股、排名和区间涨跌幅。请求失败时通过 `Result` 返回错误。



用 `with_topic_name` 变更题材名称，排序和栏目按具体题材查询场景调整。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.topic_stocks)
