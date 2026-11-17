# F10 客户端与历史资金流示例

需要 `features = ["f10"]`；不需要同时启用公开模型的 serde feature。这些调用示例会进入后续 rustdoc 编译门禁，但当前未编译、未运行。函数借用调用者配置的 client；调用者须在成功或失败后显式 close，业务错误与清理错误分别记录。F10 的来源、动态列和业务状态不能只从 HTTP 成功码推断。

## F10Client：具名入口

独立 F10 client 不会使用 HQ 连接代取 HTTP 数据。这里的 MSFT 只是显式 ticker，示例不会自动选择或探测真实服务器；实际端点由调用者通过 builder 配置。

```no_run
use eltdx::{OperationOptions, Result};
use eltdx::f10::{F10Client, MgNameQuery, UsTicker};
fn company_name(client: &F10Client, options: &OperationOptions) -> Result<()> {
    let query = MgNameQuery::new(UsTicker::new("MSFT")?);
    let response = client.mg_name(&query, options)?;
    println!("business_ok={} response={response:?}", response.ok());
    Ok(())
}
```

## F10Client：显式底层请求

只有已知 Entry 与对应的 checked query 才应交给底层 call。Body/Params 是互斥枚举，不接受两套模糊参数。动态表中的重复列、重复原始键和数字 lexeme 由类型保留。

```no_run
use eltdx::{OperationOptions, Result};
use eltdx::f10::{CallQuery, F10Client};
fn raw_call(client: &F10Client, query: &CallQuery, options: &OperationOptions) -> Result<()> {
    let response = client.call(query, options)?;
    println!("{response:?}");
    Ok(())
}
```

## MoneyFlowHistoryService：反转后截取

历史窗口按 COMP-050 先反转响应行，再按 start/count 截取，不擅自按日期排序。缺失或非法日期、非有限数字和未知金额单位保持可诊断。HTTP 正常但服务业务码失败的 Response 仍应由调用者检查。

```no_run
use std::num::NonZeroU32;
use eltdx::{OperationOptions, Result};
use eltdx::f10::{AshareCode, F10Client, F10MarketCode, MoneyFlowHistoryQuery};
fn history(client: &F10Client, options: &OperationOptions) -> Result<()> {
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    let query = MoneyFlowHistoryQuery::new(code).with_days(20)?
        .with_start(0).with_count(NonZeroU32::new(5))?;
    let series = client.money_flow_history().history(&query, options)?;
    println!("business_ok={} rows={}", series.ok(), series.rows().len());
    Ok(())
}
```

`F10Client::builder()` 创建独立 owner；`TdxClient::f10()` 是与其 HQ client 共用 owner 的句柄，两者的 close 影响范围不同。压缩 payload、HTTP 明文 wire 和解码后 JSON 是不同数据，不将 TLS 密文伪称为已捕获 wire。
