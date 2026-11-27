---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 资金流向日数据

## 作用

查询一只证券最近的日资金流向记录，包括不同口径的资金流数据。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.money_flow().daily(&id, &options);
    let closed = client.close();
    for day in result?.records().iter().take(5) {
        println!("{:?} 主力净额 {:?}", day.date(), day.main_net());
    }
    closed
}
```

### 其他调用

`daily` 读取单只股票，`daily_many` 用相同操作选项批量查询。在关闭客户端前调用。

```rust
let batch = client.money_flow().daily_many(&[id], &options)?;
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `id: &SecurityId` | 要查询的证券。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

## 返回

`daily` 返回 `Result<DailyMoneyFlow>`。`daily_many` 接受证券列表，返回 `Result<BatchOutcome<DailyMoneyFlow>>`，应逐项检查结果。`records()` 取得日记录。每条可读取 `date()`、`total_amount()` 和 `main_net()`。日期可能缺失，金额与比例分别使用各自的类型。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.MoneyFlowService.html#method.daily)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
