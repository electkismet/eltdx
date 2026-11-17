# HQ 逐服务使用示例

这些示例是候选源码，尚未通过统一编译或运行验收。它们也作为 crate rustdoc 的源码收录，默认空 feature 即可使用。`no_run` 表示后续文档门禁会编译调用，但不会连接真实服务器。

各函数借用调用者提供的 client、合法证券身份和操作选项。调用者负责显式配置端点，并在函数返回成功或失败后调用 `client.close()`；共用 client 的方法不能擅自关闭其他使用者的 owner。完整的业务错误/清理错误处理见 crate 首页和 `hq_query`。证券身份可用 `SecurityId::new(HqMarket::Shanghai, "600000")?` 构造。

返回值中的单位、来源、完整性和 missing 状态都是合同的一部分；下面用 Debug 展示结构，不把未知数值单位猜成股、手或元。每个示例对应的真实非空回环源码列在 `service-examples.md`，当前都未运行。

## CodeService：目录单页

`list` 返回一页，不表示完整市场目录；全量业务使用有界 scan，并检查结束证据和 continuation。

```no_run
use eltdx::{CodeQuery, HqMarket, OperationOptions, Result, TdxClient};
fn directory(client: &TdxClient, options: &OperationOptions) -> Result<()> {
    let query = CodeQuery::new().with_limit(10)?;
    let page = client.codes().list(HqMarket::Shanghai, &query, options)?;
    println!("securities={} end={:?}", page.items().len(), page.end());
    Ok(())
}
```

## QuoteService：保留输入 occurrence

即使只查询一个身份，也检查 batch 中的 `Present` 或明确缺失；不把缺失证券替换成零价格。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
fn snapshot(client: &TdxClient, id: SecurityId, options: &OperationOptions) -> Result<()> {
    let batch = client.quotes().snapshots(&[id], options)?;
    for item in batch.items() {
        println!("index={} security={} state={:?}", item.input_index(), item.security(), item.state());
    }
    Ok(())
}
```

## PinnedQuoteService：固定连接刷新

先由调用者 `client.pin(options)?` 取得独占 guard，再借用其 quote service。guard 不可 Clone，使用结束后应观察 `pin.close()` 的结果；不得同时让别的读取器消费这条连接的推送。

```no_run
use eltdx::{OperationOptions, PinnedSession, Result, SecurityId};
use eltdx::types::RefreshRequest;
fn refresh(pin: &PinnedSession, id: SecurityId, options: &OperationOptions) -> Result<()> {
    let request = RefreshRequest::new(id);
    let updates = pin.quotes().refresh(&[request], options)?;
    println!("{updates:?}");
    Ok(())
}
```

## KlineService：未复权日线单页

单页行序、源交易日和显示标签应分别保留。需要本地复权或 End 标签时显式选择 query；不要重写源交易日来配合显示标签。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
use eltdx::types::{AdjustMode, KlinePeriod, KlineQuery};
fn bars(client: &TdxClient, id: &SecurityId, options: &OperationOptions) -> Result<()> {
    let query = KlineQuery::new().with_period(KlinePeriod::Day)?
        .with_adjust(AdjustMode::None)?.with_count(10)?;
    let result = client.klines().page(id, &query, options)?;
    println!("{result:?}");
    Ok(())
}
```

## MinuteService：显式历史日期

历史日期由调用者提供，不使用机器今天替代缺失的源日期。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient, TradingDate};
fn minutes(client: &TdxClient, id: &SecurityId, date: TradingDate,
    options: &OperationOptions) -> Result<()> {
    let result = client.minutes().history(id, date, options)?;
    println!("{result:?}");
    Ok(())
}
```

## TradeService：逐笔单页

单页和全量 collect 是不同操作；状态、源顺序、绝对位置和成交量单位不能只投影为一个价格数组。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
use eltdx::types::TradeQuery;
fn trades(client: &TdxClient, id: &SecurityId, options: &OperationOptions) -> Result<()> {
    let query = TradeQuery::new().with_count(20)?;
    let page = client.trades().page(id, &query, options)?;
    println!("{page:?}");
    Ok(())
}
```

## AuctionService：竞价源序列

保留竞价点与正式撮合的区别；这里不把最近一个竞价快照当成正式撮合价。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
use eltdx::types::AuctionQuery;
fn auction(client: &TdxClient, id: &SecurityId, options: &OperationOptions) -> Result<()> {
    let series = client.auctions().series(id, &AuctionQuery::new(), options)?;
    println!("{series:?}");
    Ok(())
}
```

## MoneyFlowService：HQ 日资金流

此服务使用 HQ 日资金流响应；F10 历史窗口是另一个 feature 下的独立服务。原始业务字段和未知金额单位保留在结果中。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
fn money_flow(client: &TdxClient, id: &SecurityId, options: &OperationOptions) -> Result<()> {
    let result = client.money_flow().daily(id, options)?;
    println!("{result:?}");
    Ok(())
}
```

## ResourceService：有界范围读取

这是远端资源路径，不是本地文件路径。范围结束和资源 EOF 分开；完整下载应检查 completion，不能用达到 byte cap 充当完成。

```no_run
use eltdx::{OperationOptions, Result, TdxClient};
fn resource(client: &TdxClient, path: &str, options: &OperationOptions) -> Result<()> {
    let result = client.resources().read(path, 0, 4096, options)?;
    println!("{result:?}");
    Ok(())
}
```

## CompanyInfoService：公司信息目录

目录中返回的受校验文件名/范围可供后续 content query 使用。GBK 解码诊断与原始来源不能因为打印文本而丢弃。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
fn company(client: &TdxClient, id: &SecurityId, options: &OperationOptions) -> Result<()> {
    let sections = client.company_info().category(id, options)?;
    println!("{sections:?}");
    Ok(())
}
```

## SessionService：显式握手诊断

`ping()` 是本地应答，不能证明服务连通；显式 handshake/heartbeat 才执行相应网络工作。握手源日期不是无条件有效的交易日证明。

```no_run
use eltdx::{OperationOptions, Result, TdxClient};
fn session(client: &TdxClient, options: &OperationOptions) -> Result<()> {
    let result = client.session().handshake(options)?;
    println!("{result:?}");
    Ok(())
}
```

## HealthService：实际探活

probe 观测当前操作及其端点，不承诺全部连接槽或未来请求可用。

```no_run
use eltdx::{OperationOptions, Result, TdxClient};
fn health(client: &TdxClient, options: &OperationOptions) -> Result<()> {
    let report = client.health().probe(options)?;
    println!("{report:?}");
    Ok(())
}
```

## CorporateService：财务 batch

结果保留原输入 occurrence 与缺失状态；财务字段原始 f32 位、单位及换算后视图分别可查。

```no_run
use eltdx::{FinanceQuery, OperationOptions, Result, SecurityId, TdxClient};
fn finance(client: &TdxClient, id: SecurityId, options: &OperationOptions) -> Result<()> {
    let result = client.corporate().finance(&[id], &FinanceQuery::new(), options)?;
    println!("{result:?}");
    Ok(())
}
```

## LimitService：特殊限价单页

服务端特殊限价不等于完整权威涨跌停规则。无独立依据的通用规则保持 UnknownRule，不能从旧实现的冲突值推定。

```no_run
use eltdx::{OperationOptions, Result, SpecialLimitQuery, TdxClient};
fn limits(client: &TdxClient, options: &OperationOptions) -> Result<()> {
    let page = client.limits().special(&SpecialLimitQuery::new(), options)?;
    println!("{page:?}");
    Ok(())
}
```

## CalendarService：带覆盖范围的日历

refresh 要求完整源序列；结果的 coverage 以实际观察到的交易日范围为界，不能把工作日近似升级为交易所开市事实。

```no_run
use eltdx::{CalendarQuery, OperationOptions, Result, TdxClient};
fn calendar(client: &TdxClient, options: &OperationOptions) -> Result<()> {
    let snapshot = client.calendar().refresh(&CalendarQuery::new(), options)?;
    println!("days={} coverage={:?}..{:?}", snapshot.days().len(),
        snapshot.coverage_start(), snapshot.coverage_end());
    Ok(())
}
```

## HelperService：共享预算的组合查询

必需 snapshot 与可选 depth 使用同一父操作预算；可选源失败按诊断降级，取消与父期限不能被吞掉当成缺失。

```no_run
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
fn full_quote(client: &TdxClient, id: SecurityId, options: &OperationOptions) -> Result<()> {
    let result = client.helpers().full_quotes(&[id], options)?;
    println!("{result:?}");
    Ok(())
}
```
