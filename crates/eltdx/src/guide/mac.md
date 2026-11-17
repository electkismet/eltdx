# MAC 与 MAC-EX 独立客户端示例

需要 `features = ["mac"]`，该 feature 自动启用 `ex`。两种 client 有不同握手、身份和 query 命名空间，下面分别使用。所有示例当前未编译、未运行；后续 rustdoc 只编译，不执行真实网络调用。

## MacClient：字段位图报价

使用受校验的 MAC 身份，默认字段集合由 query 明确携带；返回的未知 slot、原始整数/f32 位和来源不能被一个统一 f64 表格吞掉。调用者配置独立端点和所需 setup policy，并在函数成功或失败后显式 close。

```no_run
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, MacInstrumentId, QuotesQuery};
fn quotes(client: &MacClient, id: MacInstrumentId, options: &OperationOptions) -> Result<()> {
    let query = QuotesQuery::new(vec![id])?;
    let result = client.quotes(&query, options)?;
    println!("{result:?}");
    Ok(())
}
```

## MacExClient：使用 EX 身份

MAC-EX query 接受 canonical EX 身份，内部共享 MAC 传输和响应实现，但不能直接把普通 MAC query 传给它。生产使用前，调用者必须提供经独立服务证据确认的 LoginReplyRules；此例借用已经按该条件配置的 client，不提供伪造的生产默认或合成 ACK。

```no_run
use eltdx::{OperationOptions, Result};
use eltdx::ex::ExInstrumentId;
use eltdx::mac::MacExClient;
use eltdx::mac::ex_query::QuotesQuery;
fn quotes(client: &MacExClient, id: ExInstrumentId, options: &OperationOptions) -> Result<()> {
    let query = QuotesQuery::new(vec![id])?;
    let result = client.quotes(&query, options)?;
    println!("{result:?}");
    Ok(())
}
```

两个 client 的 clone 分别共享自身不可逆关闭 owner，独立 builder 创建另一个 owner。取消或 close 未确认时保留登记资源和租约，不能把超时返回当成 socket 已释放。MAC optional setup 的响应 identity/order/fence 与 MAC-EX 登录判定都需要独立生产证据；本机正负例只验证显式合成 profile 的状态机。
