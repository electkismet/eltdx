# 独立 EX 客户端示例

需要 `features = ["ex"]`；构建和运行不需要 Python。此例当前未编译或运行，后续 rustdoc 仅编译 `no_run` 代码。

## ExClient：具名市场和目录

调用者先以 `ExClient::builder().with_endpoints(...)` 配置明确数字端点，再将 client 借给查询函数；EX 使用自身握手和会话，不把它与 HQ 的 market ID、metadata 或连接混用。

```no_run
use eltdx::{OperationOptions, Result};
use eltdx::ex::{ExClient, InstrumentsQuery};
fn directory(client: &ExClient, options: &OperationOptions) -> Result<()> {
    let markets = client.markets(options)?;
    println!("{markets:?}");
    let query = InstrumentsQuery::new(0).with_count(10)?;
    let page = client.instruments(&query, options)?;
    println!("rows={} completion={:?} next={:?}",
        page.rows().len(), page.completion(), page.next_offset());
    Ok(())
}
```

这两个公开方法分别是两次高层操作，每次使用自己的 OperationOptions 入口时钟；在一个方法内部完成的子请求才共用该方法的原始期限。若应用要限制两个方法合计的时间，可给两者同一个绝对 deadline。不要仅因复用相同 options 对象就认为相对 timeout 会跨方法累计。

查询函数返回后，无论成功或失败，拥有 client 的调用者都应观察 `close()` 结果；完整代码见 `ex_query` example。clone 共享不可逆关闭 owner，旧 service/结果来源不能被新 owner 的 generation 覆盖。目录页的 completion/next offset 必须保留，短页不普遍等于完整源 EOF。
