---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 指定日期历史分时

## 作用

查询一只证券在指定交易日的分钟走势。可用日期范围取决于主站提供的数据。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient, TradingDate};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let date = TradingDate::new(2025, 1, 2)?;
    let result = client.minutes().history(&id, date, &options);
    let closed = client.close();
    let series = result?;
    println!("返回 {} 个分时点", series.points().len());
    closed
}
```

### 其他调用

`history` 查一只证券。`history_many` 用相同交易日批量查询，读取 `items()` 查看每只证券的结果。在关闭客户端前调用。

```rust
let batch = client.minutes().history_many(&[id], date, &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `id: &SecurityId` | 要查询的证券。 |
| `date: TradingDate` | 查询日期，使用 `TradingDate::new(年, 月, 日)` 创建。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

## 返回

`history` 返回 `Result<MinuteSeries>`。`history_many` 可对多只证券查询同一天，返回 `Result<BatchOutcome<MinuteSeries>>`，每项结果需要分别检查。

用 `points()` 访问数据点。每个点对应服务器返回的一段时间。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.MinuteService.html#method.history)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
