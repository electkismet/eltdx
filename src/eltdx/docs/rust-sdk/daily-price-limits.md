---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 每日涨跌停价

## 作用

按不复权日线、当日权息和市场板块规则计算当前交易日涨跌停价格。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{DailyLimitsQuery, TradingDate};

fn main() -> Result<()> {
    let query = DailyLimitsQuery::new(TradingDate::new(2025, 1, 2)?);
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().daily_price_limits(&query, &options);
    let closed = client.close();
    for row in result?.rows().iter().take(5) {
        println!("{} 涨停 {:?} 跌停 {:?}", row.id(), row.upper(), row.lower());
    }
    closed
}
```

## 参数

创建查询时填写交易日期，示例日期应替换为需要查询的交易日。默认全部 A 股，可用 `with_selection` 指定证券。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

每行可用 `upper()`、`lower()` 查看涨停价和跌停价，用 `status()`、`rule()` 查看计算状态和规则。价格可能缺失。请求失败时通过 `Result` 返回错误。



价格缺失时可结合 `status()`、`reference()` 和 `reference_date()` 判断参考价及计算状态。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/helpers/struct.HelperService.html#method.daily_price_limits)
