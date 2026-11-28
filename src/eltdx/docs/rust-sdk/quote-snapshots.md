---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 批量行情快照

## 作用

按证券代码获取当前行情快照。一次可以传入多个证券，返回结果保留每个输入的顺序与状态。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.quotes().snapshots(&[id], &options);
    let closed = client.close();
    for item in result?.items() {
        match item.state() {
            eltdx::types::BatchItemState::Present(quote) => {
                println!("{} 最新价 {}", item.security(), quote.last().as_f64());
            }
            other => eprintln!("{}：{other:?}", item.security()),
        }
    }
    closed
}
```

### 其他调用

默认缺失策略是 `Error`。接受部分结果时使用 `MissingPolicy::AllowPartial`，再逐个检查状态。在关闭客户端前调用。

```rust
let batch = client.quotes().snapshots_with_policy(
    &[id], eltdx::types::MissingPolicy::AllowPartial, &options,
)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `ids: &[SecurityId]` | 要查询的证券列表。示例通过 `"sh600000".parse()` 创建上海证券身份。 |
| `options: &OperationOptions` | 本次操作的选项。 |
| `policy: MissingPolicy` | 仅 `snapshots_with_policy` 使用，用于指定遇到缺失数据时的处理方式。 |

## 返回

方法返回 `Result<SnapshotBatch>`。成功后用 `items()` 遍历，每项的 `security()` 是证券身份，`input_index()` 是输入位置，`state()` 表示行情存在或确认缺失。请根据状态处理缺失数据。

[查看快照方法和类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.QuoteService.html#method.snapshots)

## 出错与关闭

网络、超时或参数错误会通过 `Result` 返回。示例先保存查询结果并关闭客户端，再用 `?` 检查查询结果。
