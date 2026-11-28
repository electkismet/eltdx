---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 最新停牌列表

## 作用

根据股票列表和报价状态筛选停牌 A 股。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().latest_suspended(None, &options);
    let closed = client.close();
    let securities = result?;
    for item in securities.items().iter().take(5) {
        println!("{}", item.name());
    }
    closed
}
```

## 参数

`None` 查询 SDK 支持的所有市场，也可以传入 `Some(HqMarket)` 选择市场。`options` 控制本次操作，示例使用默认选项。

## 返回

返回 `Collected<Security>`。判断停牌所需报价缺失时返回错误。请求和参数错误通过 `Result` 返回。

证券的 `id()` 可用于后续行情查询。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.latest_suspended)
