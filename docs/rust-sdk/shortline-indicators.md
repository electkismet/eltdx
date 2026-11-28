---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 短线指标

## 作用

按交易日安全对齐统计资源、实时行情、近 5 日 K 线和财务快照，返回竞价、开盘量比、流通股本、封单和连板字段。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};
use eltdx::types::{ShortlineQuery};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let query = ShortlineQuery::new(vec![id])?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().shortline_indicators(&query, &options);
    let closed = client.close();
    let table = result?;
    for row in table.rows().iter().take(5) {
        println!("统计日期 {:?}，连板梯队 {:?}", row.stats_date(), row.ladder_level());
    }
    closed
}
```

## 参数

传入非空证券列表。`with_stats` 设置统计文件，`with_refresh_stats(true)?` 刷新统计数据。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

结果包含竞价、量比、流通股本、封单和连板等字段，并保留来源及日期对齐情况。请求失败时通过 `Result` 返回错误。



交易日和统计日期可能不同。读取带 `Option` 的字段时保留缺失值。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/helpers/struct.HelperService.html#method.shortline_indicators)
