---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 行情订阅接口

## 作用

返回订阅句柄，通过句柄接收行情事件。使用 SubscriptionOptions 配置订阅。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, PushEvent, Result, SecurityId, SubscriptionOptions, TdxClient};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let mut subscription = client.quotes().subscribe(&[id], &SubscriptionOptions::default(), &options)?;
    let event = subscription.next_event(&options);
    let stopped = subscription.close();
    let closed = client.close();
    match event? {
        Some(PushEvent::Initial(batch) | PushEvent::Resynchronized(batch)) => {
            println!("快照共 {} 项", batch.len());
        }
        Some(PushEvent::Update(update)) => println!("行情更新：{update:?}"),
        Some(PushEvent::Gap(gap)) => eprintln!("数据缺口：{gap:?}"),
        _ => {}
    }
    stopped?;
    closed
}
```

## 参数和使用

`ids` 是证券列表。`SubscriptionOptions::default()` 默认队列最多 1024 个事件、64 MiB，每 7 秒续订一次。可通过 `with_queue_limits`、`with_renew_interval` 和 `with_gap_policy` 调整。

## 返回

`next_event` 等待并返回一个 `PushEvent`，关闭后返回 `None`。`Initial` 是初始快照，`Update` 是变更，`Gap` 指出中间有数据缺口，`Resynchronized` 提供重新同步后的快照。默认仅报告缺口，可设置 `GapPolicy::Resynchronize` 自动尝试重新同步，或 `GapPolicy::Close` 发现缺口时关闭订阅。处理完毕调用 `subscription.close()`，再关闭客户端。 网络和参数错误通过 `Result` 返回。



[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.QuoteService.html#method.subscribe)
