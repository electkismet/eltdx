# Rust SDK 使用与行为指南

当前是未发布、尚未通过统一验证的候选源码。以下示例对应实现入口，最终以编译、回环、兼容及平台证据验收；不能把文档中的命令当作执行记录。架构与完整签名分别见[架构](architecture.md)、[公共 API](public-api.md)及[分域规格](extension-api-spec.md)。

## 两种语言独立消费

Rust 项目只添加 Rust 库；Python 项目只安装匹配平台的 Python wheel。共享发生在维护者的源码层：Python wheel 内包含 PyO3 及编译后的共用 Rust 业务代码。运行时没有 Python↔Rust 网络服务，也没有要求 Rust 程序启动 Python。CLI/HTTP/MCP 是另外的消费者，真正 async API 属于后续设计，不是同步 SDK 的隐含能力。

SDK 候选版本为0.1.0，最低 Rust1.89，edition2021。注册表名称、版本及发布权仍需核实；发布前从已冻结的候选源码消费：

```toml
[dependencies]
eltdx = { path = "/absolute/candidate/crates/eltdx", default-features = false }
```

Windows 可使用正斜杠绝对路径。路径是本机候选的实际位置，不能直接复制为 crates.io 已可安装的声明。独立消费的最终证据还要求工作区外的独立 manifest、lock 和 target，不只是在仓库内运行一个 example。

| Feature | 用途 | 依赖边界 |
| --- | --- | --- |
| 默认空集合 | 全部 HQ 服务和仅依赖 HQ 的 helper | 不启用 F10/EX/MAC/本地公开模块 |
| serde | 明确实现 Serde 的类型 | 不承诺所有 DTO 都支持 Deserialize |
| offline | day/minute/EX 本地文件、行业/板块/股本、财报 DAT/ZIP 与财报服务 | 显式文件 I/O，不自动扫描用户目录 |
| f10 | HTTP/TQLEX、动态表、157个具名入口及相应组合 | 自有 HTTP worker；不通过 HQ 代取 F10 |
| ex | EX 查询、配置和会话 | 与 HQ 连接隔离 |
| mac | MAC/MAC-EX、位图、目录及组合 | 自动包含 ex；MAC-EX 登录证据独立 |
| full | 上述五个 feature | 测试 full 不代替测试各子集 |

```rust
use eltdx::{Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let service = client.codes();
    drop(service);
    client.close()?;
    Ok(())
}
```

`build()` 和普通 service getter 只构造/校验，不启动 DNS、连接或后台 runtime。此例只验证惰性构造，不能证明网络查询成功。

## 实际查询与关闭

网络示例在 `crates/eltdx/examples/hq_query.rs`，要求显式数字地址和至少一个证券代码。它调用 `full_quotes`，打印单位、来源和诊断，成功或业务错误都执行 close。EX 示例为 `ex_query.rs`，财报本地/下载示例分别为 `financial_read.rs`、`financial_download.rs`。源码完成后的运行模板：

```sh
cargo run -p eltdx --example hq_query -- IP:PORT SECURITY_ID
cargo run -p eltdx --example ex_query --features ex -- IP:PORT
cargo run -p eltdx --example financial_read --features offline -- /absolute/report.zip
cargo run -p eltdx --example financial_download --features offline -- IP:PORT gpcw20240331.zip
```

本地回环验收不需要真实服务器；只有这些显式网络示例使用调用者给定的服务地址。并发应用应将同步调用安排在线程边界上，不能把同步函数直接放入 async 函数后宣称非阻塞。包装任务的取消也不等于底层请求已被取消。

`TdxClient::clone()` 共享 owner、缓存和关闭状态。service、pin、subscription 关联该 owner；关闭任意 clone 会影响全部关联句柄。Rust 关闭成功后永久拒绝新工作，需用新 builder 创建独立会话。pin 不可作为普通 clone 复制，独占期间不能让另一个路径读取同一推送流。显式 close 返回清理结果；Drop 仅是兜底。

业务失败和清理失败须分别保留：先保存业务 Result，再执行 close，再决定主错误。`CancelUnconfirmed` 的 source 保留取消/超时根因，`cleanup_failure()` 保存清理失败；不能把它记录成“已释放全部资源”。`CloseTimeout` 后可以重试确认清理，不能复用已封闭 owner 继续查询。共享 HQ/F10 client 的 F10 句柄遵从共同 owner；单独 `F10Client::builder()` 则拥有独立会话。

## 操作预算与错误

`OperationOptions::with_timeout` 从公开方法进入时起算；绝对 deadline 与相对 timeout 同时指定时取较早者。启动、排队、元数据预取、分页、补缺和重试共用一份总预算，不为每页续期。显式零 timeout 立即过期；溢出为参数错误。清理允许另用已配置的短预算，因此返回时间可能晚于业务 deadline。取消 token 不可恢复，clone 共享信号。

错误检查用 `ErrorKind`、上下文和标准 `source()` 链，不解析显示字符串。非穷尽枚举匹配需保留兜底分支。参数错误、未知/不支持、数据未就绪、确认缺失、预算耗尽、协议错误和连接失败不是同一状态；可选补充源失败是否降级由该接口合同决定。Rust 取消与总期限不能作为普通可选缺失吞掉。

## 顺序、完整性和来源

批量结果逐输入 occurrence 标识位置和证券，重复输入不是丢弃重复的授权。明确缺失使用 `ConfirmedMissing`，不能用零价格或空对象代替。单页、已收集窗口和全量完整性分开；检查 completion/end evidence/cursor。只有接口合同允许时短页才证明结束；查询窗口结束不等于整个数据源 EOF。

cursor 绑定 owner、查询及必要的来源/metadata 版本，不跨 client 或更换条件盲用。保留 per-page/per-response sources，不能把最后一次 endpoint 复制到全部历史结果。未知证券分类、未验证日期、部分行和服务端业务失败分别保留诊断。F10 的 HTTP 成功可能包含业务错误响应，仍需检查 `Response::ok()`、业务码和各结果集。

## 单位、时间、raw 和序列化

价格可能是有十进制 scale 的整数，也可能是源 f32/f64；`as_f64()` 是便利视图。精确换算用已证实的来源精度和 checked 转换，需舍入时显式指定模式。成交量可能为股、手、债券或未知，金额可能为元、千元、万元或未知；未知不能按100股/手或人民币猜算。查看 source value/unit/provenance，而不只看显示值。

时间区分上海有日期时刻、本地有日期时刻、仅日期、仅时刻、未知 raw。没有日期就不能凭机器今天补成交易时间；K线标签的 End 跨日不改变除权所用源交易日。F10 缺省 civil date 使用调用时本地日期策略，不能偷偷套用 HQ 上海时钟。日历的已知交易日与 weekday 近似分开。

RawPolicy 默认为 Omit。RetainPayload 保存协议载荷，RetainWire 在可用传输上增加收到的帧；F10 payload 为编码后的 HTTP body，wire 为收到的 HTTP 明文，均不等于解压后的 JSON，也不等于 TLS 密文。raw 策略只控制额外保留，不保证零拷贝；解码、模型和必要 provenance 仍可分配。大结果应同时配置页/记录/字节上限。

serde feature 不定义统一永久 JSON schema。只有源码上明确实现的 Serialize/Deserialize 可用；受校验的身份/日期类型保留 checked 构造，不通过任意反序列化绕过。非有限源浮点序列化报错，应用需显式映射和保留原 bits，不能悄悄写成 JSON null。具体bytes整数数组、日期/时区变体、Unknown及编译失败合同见[Serde格式与校验](examples-serde.md)。

## 稳定边界与后续阅读

MAC-EX 没有已证明的默认生产登录判定；必须注入有来源的 LoginReplyRules。可选 MAC setup 的响应身份、顺序和 fence 需要独立证据，合成 ACK 只证明本机状态机合同。category22、未知涨跌停规则、未知单位与未采样市场不能扩大为稳定承诺。

版本和包见[发布政策](release-policy.md)，逐服务普通调用与非空回环对应见[示例索引](service-examples.md)，现有 Python 使用者见[兼容迁移](python-migration.md)，要求与证据逐项关系见[最终验收索引](final-acceptance.md)，维护与复验见[维护手册](maintenance.md)。
