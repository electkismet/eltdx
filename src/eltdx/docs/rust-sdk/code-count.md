---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 证券代码数量

## 作用

统计某个行情市场的证券代码数量。`count` 返回主站提供的全部代码数，包括该市场的其他证券类型。`a_share_count` 则扫描目录并统计 A 股。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{HqMarket, OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.codes().count(HqMarket::Shanghai, &options);
    let closed = client.close();
    println!("上海市场代码数量：{}", result?);
    closed
}
```

### 其他调用

`count` 返回该市场的代码总数。只统计 A 股时用 `a_share_count`，它实际扫描并筛选目录，受 `CodeScanQuery` 的分页预算约束。在关闭客户端前调用。

```rust
let scan = eltdx::types::CodeScanQuery::new();
let a_shares = client.codes().a_share_count(HqMarket::Shanghai, &scan, &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `market: HqMarket` | 选择市场，例如 `HqMarket::Shanghai`。 |
| `options: &OperationOptions` | 本次请求的超时等操作选项。 |
| `query: &CodeScanQuery` | 仅 `a_share_count` 使用，用于配置扫描分页和预算。 |

## 返回

`count` 返回 `Result<u32>`，成功时是代码数量。`a_share_count` 返回 `Result<usize>`。A 股统计需要扫描目录，因此要考虑分页预算和请求耗时。

[查看所有方法和类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.CodeService.html)

## 出错与关闭

网络、超时或参数错误会通过 `Result` 返回。示例先保存查询结果并关闭客户端，再用 `?` 检查查询结果。
