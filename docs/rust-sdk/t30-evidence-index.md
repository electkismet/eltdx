# T30 压力、性能与最终交付证据索引

T01–T30源码范围已写齐，stage115后进入统一验收。stage134已在干净候选588d5e4上完成Windows/Linux默认release压力：两平台各10场景、独立verify及跨平台aggregate均exit0，原始采样与候选hash见[统一验证记录](unified-validation.md)。性能测量尚待执行；下面的工作量和资源政策保留为事前合同。stage114在stage113的指南和R01–R12导航基础上补齐18个service、独立客户端的普通调用示例与精确非空回环对应，以及E06 Serde合同和严格单feature文档门禁。

## 独立 Rust 压力源码

`crates/eltdx/tests/stress.rs`、`stress_mac.rs` 中的用例全部显式 ignored；`src/stress_internal.rs` 仅通过 `cfg(test)` 加入私有 library test。SDK 生产 API 不增加测试诊断入口。所有服务器均为本机合成服务器；MAC-EX 显式配置的正负登录字节不构成生产证据。

| 原始日志 case | 默认工作量 | 实际路径与断言 |
| --- | --- | --- |
| unique | 125000 个业务 ID、8 callers、pool4 | 每五次预取消一次，其余 100000 次实际目录请求；核对非空返回的业务身份，轮换 Omit/RetainWire；另有128次预热，不计业务工作量 |
| inflight | 1000 对取消/恢复、pool1 | 确认服务器已收到才取消；释放迟到响应后查询另一身份，不能将旧结果交付给新操作 |
| close | Linux10000、Windows2000 个 owner | 每次实际查询、pin/close、clone/repeat close；旧 service 不能重开，要求零活动 fixture socket、累计物理连接数准确 |
| subscription | 1000 个订阅、queue2 | 三条 push 同时到达，暂停读取造成 gap1，保留两条均标 desynchronized；每轮 close 后重新获取 pin |
| limits | 1000 轮 | 无终点目录在4页/4行边界失败；持续坏公司目录为 Protocol；30000字节下载明确 LimitReached，不冒充 EOF |
| retry | 1000 个业务 ID | 每次首次请求强制断开，有限重试必须换物理连接，成功结果仍属于原业务 ID |
| financial | 1000 轮、256行×64字段 | 真 DAT/ZIP 解码与 raw 策略、预取消、DAT byte cap、total field allocation cap；不以最终 RSS 代替分配前限额断言 |
| f10 | 1000 个 HTTP owner | 真 POST/非空表与 raw，独立 worker/runtime、clone/repeat close、旧 handle Closed、peer EOF |
| mac_cache | 500 个 MAC-EX owner | 真实 EX 目录 count/page/count 与 MAC goods；缓存命中不得额外请求，宏观读取复用 goods，clear 后加载新快照，旧 Arc 在 clear/close 后内容不变 |
| budgets | 32 callers×1000、pool4、每请求服务器延迟1ms | 实际 SDK 目录请求；私有 seam 采集 engine raw/decoded 当前/max/peak、queue、配置 worker 和活动 actor/generation；关后预算为零 |

`hq_server.rs` 为长期压力补充物理连接 identity 与活动数，并及时 join 已完成连接线程。发生任一 fixture 错误时，accept loop 保留首错、停止并 join 所有子线程，避免测量工具自身积累线程。普通回归仍须覆盖此公共 fixture 修改。

十个场景现均先走真实预热路径，再记录独立的warmed baseline和measurement边界；配置日志保存pool/queue/页/记录/byte限制或fixture尺寸。预热不计terminal工作量，owner类另有一个预热owner，unique/budgets另有128个请求。inflight单独记录已admit取消到调用终态的admitted_cancel_confirm，close单独记录owner_close；采集器核对这两类时延的业务ID与数量，不以整轮耗时冒充。baseline/middle/cooldown OS资源取最后一次预热边界，吞吐取measurement至end，均在复验时从原始日志重算。

## 采集和复验

`scripts/verification/rust_sdk_stress.py` 由验证环境 Python3.11+/psutil 编排；这不构成 Rust SDK 的构建或运行依赖。先完整 release `--no-run --message-format=json` 编译，找到实际 library/integration 可执行文件，再逐 case 单独启动。观测对象是 Rust test 进程及其子进程，编译器和 Python 观察器不计进 SDK RSS。

采集内容包括：完整候选 SHA/tree/clean、全部 crate/script/fixture/contract 的工作区字节 hash 和 Git blob、Cargo.lock、Rust/Cargo版本、features/full/release、OS/CPU/psutil、真实命令/退出码/超时/UTC、二进制 hash、stdout、append-only NDJSON 业务/线上身份和资源采样。原始日志含系统时钟以关联采样，操作耗时使用单调时钟。OS采样包含 RSS、线程、FD或Windows handles、实际 established TCP、CPU，以及曾观察到的两端点对应 TIME_WAIT；TIME_WAIT 不当成存活 socket 或内存泄漏。采样会有观察开销，性能报告必须单列该开销。

聚合器重新打开全部原始事件与资源日志，重算业务 ID 全覆盖/唯一终态、实际 wire 次数、物理连接内请求 ID 唯一、取消比例、重试换连接、p50/p95/p99、吞吐、内部预算与 OS 峰值/冷却残留/后段增长。它要求完整 Linux+Windows、同 SHA/tree/blob/lock/脚本、默认次数、release/full/1.89.0；拒绝 dirty、缺 case/原始日志/hash 不符、非零退出、超时、采样失败或汇总与原始记录不一致。普通旧 Python 压力报告不进入此入口。

budgets原始记录另含实际actor槽数，复验要求`live_actors <= actor_slots <= pool`，每个槽的`(runtime_epoch, tcp_generation)`不回退，至少观察到真实存活代次；pending和stale事件计数必须合法。实际close后raw/decoded预算、存活actor和排队数均须为零，源码用例另检查所有残留actor快照已停止且无pending。该观测与inflight迟到响应的身份断言互补，单靠代次单调不能证明没有串线。

### 事前固定资源政策

| 项 | 上限或规定 |
| --- | --- |
| RSS峰值 | 1GiB/进程树 |
| 冷却相对基线RSS | +128MiB |
| 工作量最后两四分位RSS中位数增量 | +32MiB |
| 同区间FD/handles中位数增量 | +32 |
| 冷却线程/FD或handles/established增量 | +4 / +32 / 0 |
| 线程/FD或handles峰值 | 128 / 4096 |
| 采样间隔目标/单case超时 | 20ms / 1800s；实际采样时间以日志为准 |
| 内部预算 | 当前和peak均不超实际配置的raw/decoded上限；queue≤256；actor槽数≤pool；close后raw/decoded/存活actor/queue为0 |

这些是候选事前验收政策，不是已测结果或无泄漏证明。Windows owner循环设2000，Linux设10000，防止不同端口/TIME_WAIT行为使关闭压力变成端口分配测试；Windows仍覆盖2000次真实建连与所有同样断言，减少80%的owner循环覆盖必须在最终报告保留。不得在失败后无说明放宽阈值或减少次数。若平台无法观测系统连接/进程资源，应保留采样失败，不能填零。

统一验证阶段且基础正确性通过后，才可执行：

```sh
python scripts/verification/rust_sdk_stress.py run --implementation-complete --output artifacts/native-stress
python scripts/verification/rust_sdk_stress.py verify --implementation-complete --candidate FULL_CANDIDATE_SHA --inputs artifacts/native-stress
python scripts/verification/rust_sdk_stress.py aggregate --implementation-complete --candidate FULL_CANDIDATE_SHA --inputs artifacts/stress-inputs --output artifacts/native-stress-aggregate.json
```

每次 run 使用全新目录并要求干净候选；原失败日志不得覆盖。`rust-sdk-stress.yml` 是独立手动工作流，默认 quick CI 不运行这些 ignored 负载。聚合必须使用产生证据的候选仓库 Git objects 复核 blob，不能只相信 JSON 声称的 SHA。注册表发布、真实公网探测与上线仍是独立行动，本工具均不执行。

## 源码交接与待执行证据

- 固定语料的Rust core、旧/新Python解析和模型构造、非空loopback、实际close及预取消微负载源码已写，详见[性能政策](performance-policy.md)。84个默认trial、同release profile、两个真实构建wheel及原始日志复验入口均未执行；外部网络仅显式opt-in。
- stage113补齐了代次/资源源码记录和复验条件，stage114按T30交付范围进行源码收尾审阅；这些不替代最终压力运行。
- [R01–R12最终交接](final-acceptance.md)、crate首页rustdoc、[Rust指南](user-guide.md)、[Python迁移](python-migration.md)、[候选说明](release-notes.md)与[维护手册](maintenance.md)已有源码。stage131 Windows Rust1.89/1.100 platform matrices 已执行最小/default/full/serde/f10严格rustdoc和doctest；正式手动documentation job复用现有mirror/metadata/MkDocs/local-link检查器，需在stage132恢复记录后重跑。
- 逐公开service的普通调用源码、实际include及非空回环精确函数已补入[示例索引](service-examples.md)；[Serde](examples-serde.md)补齐有限/非有限、bytes、日期和未知值及checked类型边界。新增固定连接非空刷新与七组serde运行合同源码。后续须真实收集/编译/执行，不能以文档表或函数名代替结果。
- T19源码交接见[F10传输索引](f10-transport-index.md)，其依赖/API/MSRV/license/平台仍待执行。生产MAC-EX登录和optional MAC setup关联证据仍缺，不能把合成ACK升级为默认生产规则。

压力实际输出为 `artifacts/unified/stage134-windows-stress-attempt01`、Linux任务缓存下的 `stage134-linux-stress-attempt01`；两边可复验副本在 `artifacts/unified/stage134-stress-inputs`，聚合报告为 `artifacts/unified/stage134-linux-final/native-stress-aggregate.json`（passed）。性能输出仍待生成；不能以压力通过替代性能测量。
