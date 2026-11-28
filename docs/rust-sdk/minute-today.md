---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 当日分时

## 作用

查询一只证券当日的分钟走势。服务端返回的序列可能受交易时间和主站数据情况影响。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.minutes().today(&id, &options);
    let closed = client.close();
    let series = result?;
    println!("返回 {} 个分时点", series.points().len());
    closed
}
```

### 其他调用

`today` 查一只证券。`today_many` 用相同操作选项批量查询，读取 `items()` 查看每只证券的结果。在关闭客户端前调用。

```rust
let batch = client.minutes().today_many(&[id], &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `id: &SecurityId` | 要查询的证券，示例使用上海的 `600000`。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

## 返回

`today` 返回 `Result<MinuteSeries>`，成功时为分钟走势序列。`today_many` 接受证券列表，返回 `Result<BatchOutcome<MinuteSeries>>`，每个证券的结果需要分别检查。

用 `points()` 访问数据点。每个点对应服务器返回的一段时间。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.MinuteService.html#method.today)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
