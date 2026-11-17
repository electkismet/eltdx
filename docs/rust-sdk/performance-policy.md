# Rust / Python 性能测量与复验

本页包含事前固定的测量合同及stage135实际记录。Windows/Linux各84个默认run已完成并由独立verify复算；详细原始证据、版本和hash见[统一验证记录](unified-validation.md)。性能结果不能代替兼容或生命周期正确性，也没有预设“候选必须更快”的门槛。

## 负载和可比范围

`scripts/verification/rust_sdk_performance.py` 编排独立 release Rust example 和两个隔离 Python wheel 环境。共同语料来自不调用生产编码器的 `performance_fixture.py`：固定256行×64个finite float32字段的DAT，以及同一台本机HQ假服务器的非空目录行。每条曲线并发1、预热50次、测量500次、7个独立进程 trial，seed0；奇偶trial反转顺序，保留每个样本和真实服务器请求。实际吞吐随机器、工具链与系统变化，不在源码中预填。

| 曲线 | 实际测量内容 | 不应作出的推断 |
| --- | --- | --- |
| rust:core | Rust严格 `offline::financial::parse_dat`，Omit，已在内存的同一DAT | 不是完整文件I/O；严格策略不同，不能直接减去此数推导Python转换耗时 |
| baseline/candidate:parse | 旧/新安装wheel的公开 `parse_financial_dat`，包含原接口的解析、native边界与返回列表 | 不是只测纯Rust；不使用共享新实现生成“旧实现”基线 |
| baseline/candidate:projection | 已解码行到公开 FinancialRecord/dataclass 与fields tuple构造 | 不含DAT解析，也不代表全部PyO3 boxing；此曲线独立测DTO分配层 |
| rust/baseline/candidate:loopback | 同一fixture实现、非空目录请求、连接复用、参数/身份校验 | 不是公网延迟，也不含编译/首次安装 |
| rust/baseline/candidate:close | 每个样本先实际查询并建立owner，再测公开close | Rust永久关闭与Python可重开契约各自保留；不测旧handle重开速度 |
| rust:cancel | build与预取消后公开请求拒绝的微负载 | 无线上请求，绝不能当作已admit的取消确认耗时 |
| stress admitted_cancel_confirm / owner_close | 压力原始日志中从实际已admit请求cancel到caller终态、显式close调用耗时 | 另有队列/网络压力配置，不与单并发微负载混为同一曲线 |
| external（可选） | 显式endpoint上的独立公开目录请求，保留时间、源地址和结果 | 服务端/网络负载不可控，不支持“核心算法更快”的因果结论 |

`measure_candidate.rs` 需要 `offline` feature。Rust worker只依赖SDK/标准库，不依赖Python；Python在此负责开发测量及OS观测。Python worker只导入标准库和已安装wheel的公开API；记录真实native文件hash，要求等于候选wheel中解出的native字节。两个环境不从工作区 `src` 导入。

## 固定构建和来源

先对冻结的旧基线checkout和最终候选分别执行 `build-wheel`。候选必须干净，基线SHA必须与候选不同；两者都运行实际 `maturin build --release --locked`，由同一工具链和Python构建。记录原始完整SHA、lock hash、Rust/Cargo/Python版本、命令/退出码/log hash及wheel/native hash。禁止用debug wheel对比release Rust。

Rust worker和两个wheel均固定 release opt-level3、debug0、LTO=false、codegen-units16、incremental=false、panic=unwind，拒绝额外RUSTFLAGS；Rust worker的Cargo.lock字节须与候选wheel构建所用一致。依赖自身版本差异属于迁移候选事实，不能伪装成两个构建完全相同。最终报告保留基线和候选各自lock来源。

`build-wheel` 使用验证环境已安装的maturin，只构建，不发布。run复制原构建记录/日志/wheel供后续复验，分别创建venv，用 `pip install --no-index --only-binary=:all:` 安装指定文件；无对应wheel时失败，不回退sdist。普通Python用户不需要这些构建工具。

## 执行模板

下列命令只供全部源码完成后的统一验证使用。每次输出使用全新目录。`BASELINE_CHECKOUT`是冻结旧源码的独立目录，应先确认其完整Git SHA对应设计基线；不能在当前候选目录上回滚。旧基线初始来源为af471f1（v3.3.5），最终报告需记录完整SHA与实际构建结果。

```sh
python scripts/verification/rust_sdk_performance.py build-wheel --implementation-complete --source-root BASELINE_CHECKOUT --output artifacts/perf-baseline-build
python scripts/verification/rust_sdk_performance.py build-wheel --implementation-complete --output artifacts/perf-candidate-build
python scripts/verification/rust_sdk_performance.py run --implementation-complete --baseline-build artifacts/perf-baseline-build/wheel-build.json --candidate-build artifacts/perf-candidate-build/wheel-build.json --output artifacts/performance
python scripts/verification/rust_sdk_performance.py verify --implementation-complete --output artifacts/performance
```

公网只有显式 `run --external-endpoint IP:PORT` 才会访问；默认没有公网请求。该选项会为每个公开端点曲线执行同样的500+50次×7 trial，使用前必须确认目标允许这一负载。外网默认标unrun并与本机证据分开。

## 结果与限制

每个trial记录原始latency NDJSON、完整工作量、stdout、退出码/超时、起止UTC，外部psutil采样实际工作进程RSS/线程/CPU。Python观察器、Cargo及假服务器不纳入工作进程RSS/CPU。采样CPU为进程启动至最后可观测点的累计值，包含导入、预热、验证与输出；RSS为采样峰值，可能错过短暂分配；两者不能被改写成纯解析阶段的精确CPU或分配器峰值。压力工具另测真实内部budget峰值，两类指标互补。

汇总含p50/p95/p99、计时区域吞吐、整worker墙钟吞吐、采样资源峰值，以及7次trial的中位数/min/max波动范围。后者不是统计置信区间。Python迁移前后比率采用candidate/baseline，>1表示latency更高；不设事后“必须更快”的选样门槛，不删除慢trial，不将不同严格度策略的差值叫作纯转换成本。

verify重新读取样本重算分位数、吞吐、资源峰值和trial比较；要求完整84个默认trial、无重复/缺失ID、固定fixture字节、准确14字节目录请求和wire offset序列、实际native匹配、未修改的构建记录/日志/wheel/lock/可执行文件/源码快照。实际执行的worker.py必须匹配候选脚本快照，快照内容还须匹配Git中候选blob，不能只比较报告自己声称的hash。loopback要求全部请求复用一个物理连接，close要求每次建立独立物理连接；连接以fixture单调编号区分，不以可能重用的临时端口冒充identity。

资源日志在关闭文件后计算hash，包含末尾缓冲数据；服务器错误、实际流量和已产生的worker记录须先持久化，再进行结果判定。失败原样保留，报告为failed而非measured。源码有任何修复，先形成新候选，再重建受影响的两端交付物和证据，不能将不同SHA混成一个发布结论。

手动 `rust-sdk-performance.yml` 独立于默认quick CI及压力工作流；Windows/Linux分别测量，不能跨机器直接除出速度比。stage135使用固定脚本在两平台执行了配对测量，结果见下节；该手动GitHub Actions workflow本身尚未触发。

## Stage135 实际结果

候选`a3cc4c2317f4714b0e80de29d8f899aef664a59b`，冻结基线`af471f1adf38757f6aa0188968db8110eb23f6ad`。Windows用Python3.12.6/MSVC，Linux Ubuntu22.04用Python3.10.12；两者Rust1.89。每个平台均500 measured samples、50 warmup、7 trial/曲线，共84 run，原始复算0错误，外部endpoint为unrun。profile严格遵守本文锁定配置。两份lock hash各自保留，候选和基线依赖不同不隐去。

| 平台 | parse p50 比率 | projection p50 比率 | loopback p50 比率 | close p50 比率 |
| --- | ---: | ---: | ---: | ---: |
| Windows | 1.0043 | 1.0049 | 2.3671 | 1.0570 |
| Linux | 0.9628 | 0.9752 | 1.8178 | 1.0368 |

比率为candidate/baseline，>1代表候选延迟较高。Windows parse基线/候选p50中位数分别561700/564100ns，projection267100/268400ns，loopback150900/357200ns，close244000/257900ns。Linux分别为461780/444611ns、191385/186638ns、205006/372657ns、189130/196093ns。loopback为固定本机synthetic HQ server，差值约0.17–0.21ms；不能外推公网体验。Rust core strict DAT解析p50为Windows177200ns、Linux76834ns，但解析策略不同，不能与Python曲线相减推导转换耗时。上述只按既定政策报告，无提速通过阈值。

Windows全部run与verify见 `artifacts/unified/stage135-performance/windows-measurement/`；Linux压缩raw包 `artifacts/unified/stage135-performance/linux-measurement-attempt01.tar.gz` 的SHA256为 `639ebb1898ffb3135673d0e522c4680e0c8cc3621f7233e8d20d8fa0f75cce0e`，包内证据解包重验通过。跨平台wheel、native、lock、源码和日志hash见 `artifacts/unified/stage135-performance/results.json`。
