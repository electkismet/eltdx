---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 分时副图

## 作用

读取买卖强度或量比相关分时数据。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.minutes().aux(&"sh600000".parse()?, eltdx::types::AuxKind::BuySellStrength, &options);
    let closed = client.close();
    let series = result?;
    println!("返回 {} 个分时点", series.points().len());
    closed
}
```

## 参数

`AuxKind::BuySellStrength` 查询买卖强度，`AuxKind::VolumeComparison` 查询量比相关数据。`options` 控制本次操作，示例使用默认选项。

## 返回

返回 `MinuteAuxSeries`，可通过 `points()` 查看每个时间点的数据。请求和参数错误通过 `Result` 返回。

用 `points()` 访问数据点。每个点对应服务器返回的一段时间。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.MinuteService.html#method.aux)
