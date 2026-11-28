---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 实时涨停梯队

## 作用

从实时短线指标中筛选封板或触板股票，按涨停梯队和封单金额排序。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{LadderQuery};

fn main() -> Result<()> {
    let query = LadderQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().limit_ladder(&query, &options);
    let closed = client.close();
    for row in result?.rows().iter().take(5) {
        println!("梯队 {:?}", row.ladder_level());
    }
    closed
}
```

## 参数

默认扫描全部 A 股并仅包含封板股票。`with_selection` 指定证券，`with_include_touched(true)?` 包含触板，`with_count(Some(n))?` 限制数量。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

`rows()` 读取短线指标，包含涨停梯队、封单等字段。请求失败时通过 `Result` 返回错误。



`with_stats`、`with_refresh_stats` 调整统计资源。`with_count(None)` 不单独限制结果条数。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.limit_ladder)
