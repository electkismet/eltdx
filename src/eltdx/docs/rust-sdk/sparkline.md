---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 走势线

## 作用

读取用于简短走势图的价格序列。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{SparklineQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = SparklineQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.minutes().sparkline(&id, &query, &options);
    let closed = client.close();
    let series = result?;
    println!("基准 {:?}，共 {} 个价格点", series.base(), series.prices().len());
    closed
}
```

## 参数

`SparklineQuery::new()` 默认窗口为 20，选择器为 1。可用 `with_window` 设置大于零的窗口，当前支持选择器 1。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `SparklineSeries`，可使用 `prices()` 读取价格序列，`base()` 读取基准价格。 网络、超时和参数错误通过 `Result` 返回。

- `MinuteService::sparkline(&self, id: &SecurityId, query: &SparklineQuery, options: &OperationOptions)`，返回 `Result<SparklineSeries>`。

保留基准价格和原始点顺序即可在图中重建走势。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.MinuteService.html#method.sparkline)
