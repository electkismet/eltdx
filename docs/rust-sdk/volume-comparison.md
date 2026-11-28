---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 分时量比

## 作用

读取分时量比相关数据。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient, SecurityId};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().volume_comparison(&id, &options);
    let closed = client.close();
    let series = result?;
    println!("返回 {} 个分时点", series.points().len());
    closed
}
```

## 参数

传入证券身份和操作选项。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `MinuteAuxSeries`，通过 `points()` 读取时间点及其前期、当前和累计量数据。 网络、超时和参数错误通过 `Result` 返回。

- `HelperService::volume_comparison(&self, id: &SecurityId, options: &OperationOptions)`，返回 `Result<MinuteAuxSeries>`。

用 `points()` 访问数据点。每个点对应服务器返回的一段时间。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.volume_comparison)
