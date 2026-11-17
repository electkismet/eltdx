# 全源码门槛与统一验证记录

## 源码门槛：stage115

日期为2026年11月17日。root在源码检查点 `23bf1c83718e83d20e9c5ba9b919e9dfac3e2db4` 保存后，确认T01–T30约定的实现、兼容绑定、测试/回环/压力/性能工具、消费者、包和文档源码范围均已写齐；普通Git状态显示工作区干净。根据用户“全部写完后统一测试”的既有指令，从本门槛开始允许统一执行验证。此决定只允许开始验证，**不证明任何编译、行为、兼容或发布验收已通过**。

源码交接：`task-30-report.md`第四检查点、[逐服务示例](service-examples.md)、[Serde合同](examples-serde.md)、[最终R01–R12索引](final-acceptance.md)、[压力](t30-evidence-index.md)与[性能](performance-policy.md)。T01–T30原计划的执行验收框仍保持未通过。

统一验证从实际工具链/解释器发现和依赖lock准备开始，随后基础编译、全部离线合同与故障回环、Python兼容、外部consumer/包、feature/MSRV/平台、文档、压力和性能。失败必须保留原始命令、退出码和日志；修正后记录新候选与需要重跑的范围，不降低测试范围或覆盖原失败。耗时进程保存实际session/输出路径，网络中断后先恢复观察原进程。

## 当前结果

此门槛提交时尚未运行任何新测试、构建、依赖解析、网络探测、格式或文档校验。没有通过计数。后续每阶段在此追加真实结果摘要和原始日志位置，完整证据仍以实际输出为准。

生产MAC-EX登录规则、optional MAC setup关联、category22非空capture、未知涨跌停规则、注册表源码身份、专用无编译器镜像及跨平台实际证据继续分别保留具体缺口，不能以本地合成通过替代。目标继续active，无额度上限。本阶段没有授权发布、push、tag或merge。

## stage116：实际工具、lock与首轮编译修正

本机实际为Windows x86_64 MSVC，PowerShell7.6.9，Rust/Cargo1.89.0，另有stable工具链；Python3.12.6。pytest9.0.2、maturin1.15.0、MkDocs1.6.1及psutil6.1.1已存在，WSL列出Ubuntu22.04，但尚未进入Linux执行验证，也未据此声明干净隔离环境。

原始日志保留在 `artifacts/unified/stage116-tooling/`，不提交生成日志。各attempt独立文件，失败未覆盖：

| 实际命令/记录 | 结果与含义 |
| --- | --- |
| `cargo +1.89.0 generate-lockfile` attempt01 | exit0，选出214包，但generator0.8.12及Windows0.100传递依赖要求Rust1.95；不能据exit0称MSRV兼容 |
| `cargo +1.89.0 check -p eltdx-runtime --features loom --locked` attempt01 | exit101，实际复现上述MSRV冲突 |
| 同一lock命令 attempt02，新增resolver fallback | exit0，选出208个符合Rust1.89的包，generator0.8.11；直接精确版本不变 |
| 同一Loom check attempt02 | exit0，仍有警告；仅编译，不是Loom行为通过 |
| `cargo +1.89.0 check -p eltdx --all-targets --no-default-features --features full --locked` attempt01–03 | exit101，保留Hyper客户端API/私有runtime启动及测试类型推断/导入/结果访问的实际错误 |
| 同一full check attempt04 | exit0，仍有警告；不是clippy或全部行为通过 |
| `f10_http::encoded_chunked_body_and_response_header_caps_apply_while_reading` | 移除不可用Hyper方法后实际出现Protocol≠LimitExceeded失败；补有界原始头观察后最终诊断attempt通过1项，26项过滤，旧失败保留 |
| `cargo +1.89.0 test -p eltdx --lib --no-default-features --features full --locked header_budget_tests` | exit0，3通过、230过滤；exact/逐边界拆包、未终结/超一字节、informational与body分界 |

根因与修正：Cargo resolver2默认接受更新的高MSRV传递版本，仓库增加 `.cargo/config.toml` 的 `incompatible-rust-versions="fallback"` 并重建lock，独立staging复制该配置，压力来源清单也包含它。Hyper1.12的 `is_parse_too_large` 受server feature限制，client-only不可用；不引入server功能，改在既有HTTP明文字节观察器用固定状态跟踪头边界、每头byte cap、informational响应和最终body分界，保留typed LimitExceeded及原错误链。Hyper仍负责完整语法与framing校验。

其余编译修正保持合同：并发close测试通过公开begin_connect启动真实runtime并保留pending；cursor和finance测试补明确类型；F10 extras接受slice；NewsChannel补实际导入；pin结果requested元素直接按SecurityId断言。没有删除用例、降低阈值或变更Python公开合同。

这些运行发生在b94d762之后的工作树修正中，正式同候选验收须在恢复提交后重跑受影响集合。完整Rust行为、PyO3编译/Python兼容、feature/平台/包/文档/压力性能仍未通过，警告仍待处理。后续继续统一验证，不将本节局部通过扩大成项目完成。
