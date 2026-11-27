---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 近期分时

## 作用

读取证券近期分时数据。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{MinuteQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = MinuteQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.minutes().recent(&id, &query, &options);
    let closed = client.close();
    let series = result?;
    println!("返回 {} 个分时点", series.points().len());
    closed
}
```

### 其他调用

`recent` 查一只证券。`recent_many` 用相同查询条件批量查询，读取 `items()` 查看每只证券的结果。在关闭客户端前调用。

```rust
let batch = client.minutes().recent_many(&[id], &query, &options)?;
```

## 参数

`MinuteQuery::new()` 使用当前日期。`with_date(MarketDateChoice::Date(date))` 可以指定交易日期。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `MinuteSeries`，通过 `points()` 读取分时点，单点提供时间、价格、成交量和可选均价。 网络、超时和参数错误通过 `Result` 返回。

- `MinuteService::recent(&self, id: &SecurityId, query: &MinuteQuery, options: &OperationOptions)`，返回 `Result<MinuteSeries>`。
- `MinuteService::recent_many(&self, ids: &[SecurityId], query: &MinuteQuery, options: &OperationOptions)`，返回 `Result<BatchOutcome<MinuteSeries>>`。

用 `points()` 访问数据点。每个点对应服务器返回的一段时间。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.MinuteService.html#method.recent)
