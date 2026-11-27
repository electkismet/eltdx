---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 证券代码列表

## 作用

读取指定市场的证券代码和名称。`list` 读取一页，`all` 在预算内自动翻页。还可以使用 `all_a_shares`、`all_etfs` 或 `all_indices` 按类型筛选。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{CodeQuery, HqMarket, OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let query = CodeQuery::new().with_limit(10)?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.codes().list(HqMarket::Shanghai, &query, &options);
    let closed = client.close();
    let page = result?;
    println!("本页数量：{}", page.items().len());
    println!("本页内容：{:?}", page.items());
    println!("结束状态：{:?}", page.end());
    closed
}
```

### 其他调用

`all` 读取某个市场。`all_a_shares`、`all_etfs` 和 `all_indices` 分别收集 SDK 支持市场内的 A 股、ETF 和指数。`with_pages` 设置扫描预算，`with_category` 可附加分类条件。再次读取时可用 `with_cursor`。在关闭客户端前调用。

```rust
let scan = eltdx::types::CodeScanQuery::new();
let shanghai = client.codes().all(HqMarket::Shanghai, &scan, &options)?;
let stocks = client.codes().all_a_shares(&scan, &options)?;
let etfs = client.codes().all_etfs(&scan, &options)?;
let indices = client.codes().all_indices(&scan, &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `market: HqMarket` | 指定市场，例如上海市场 `HqMarket::Shanghai`。 |
| `query: &CodeQuery` | `list` 的分页条件。`with_start` 设置起始位置，`with_limit` 设置每页数量，最大为 1600。 |
| `query: &CodeScanQuery` | 自动翻页方法的扫描条件，包括分页预算和筛选条件。 |
| `options: &OperationOptions` | 本次操作的选项。 |

## 返回

`list` 返回 `Result<DataPage<Security>>`，`items()` 读取本页证券，`end()` 查看结束证据。一页数据不代表市场的完整目录。`all` 返回 `Result<Collected<Security>>`，自动扫描仍受分页预算限制，应检查结果的结束状态。

[查看所有方法和类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.CodeService.html)

## 出错与关闭

网络、超时或参数错误会通过 `Result` 返回。示例先保存查询结果并关闭客户端，再用 `?` 检查查询结果。
