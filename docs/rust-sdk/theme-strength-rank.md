---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 题材强度排行

## 作用

按个股 F10 题材聚合涨停数量、最高板、连板数和封单金额。

## 示例

在 `Cargo.toml` 中添加 `eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::ThemeStrengthQuery;

fn main() -> Result<()> {
    let query = ThemeStrengthQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().theme_strength_rank(&query, &options);
    let closed = client.close();
    for row in result?.rows().iter().take(5) {
        println!("{} {}，最高板 {}", row.rank(), row.topic_name(), row.highest_ladder_level());
    }
    closed
}
```

## 参数

需启用 `f10`。`with_ladder` 设置股票筛选条件，`with_count` 限制最终排行数，`with_max_topic_records` 和 `with_max_groups` 设置处理上限。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

结果按题材汇总涨停数、最高板、连板数和封单金额。题材数据获取情况保留在结果中。请求失败时通过 `Result` 返回错误。



限制最终结果数时，聚合仍基于查询范围内的完整梯队。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.theme_strength_rank)
