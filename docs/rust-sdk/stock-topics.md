---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 个股概念板块

## 作用

合并股票基础信息和热点题材结果，整理某只股票的全部题材与概念。

## 示例

在 `Cargo.toml` 中添加 `eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::f10::{AshareCode, F10MarketCode};
use eltdx::helpers::StockTopicsQuery;

fn main() -> Result<()> {
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    let query = StockTopicsQuery::new(code);
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().stock_topics(&query, &options);
    let closed = client.close();
    let result = result?;
    for topic in result.topics().iter().take(5) {
        println!("{:?} {:?}", topic.topic_name(), topic.reason());
    }
    closed
}
```

## 参数

需启用 `f10`。传入 A 股代码，`with_max_records` 设置题材记录上限。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

使用 `topics()` 读取题材，每条可查看 `topic_id()`、`topic_name()` 和 `reason()`。请求失败时通过 `Result` 返回错误。



`ok()` 可检查各 F10 来源的业务状态。可将 `topic_id()` 交给 [概念板块成分股](topic-stocks.md)。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.stock_topics)
