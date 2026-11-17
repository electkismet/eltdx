# Rust SDK 与 Python 版本、打包及发布政策

本轮选定的是候选源码配置，尚未编译、查询注册表、打包或安装。本页不表示发布可用；所有执行等 T01–T30 源码范围写齐后统一进行。

| 产品或组件 | 候选版本 | 依赖关系与安装 |
| --- | --- | --- |
| Rust 业务 SDK `eltdx` | `0.1.0` | 用户通过 Cargo 消费；default 无扩展 feature；不依赖 Python |
| `eltdx-protocol` | `3.3.5`，沿用 workspace 版本线 | runtime 与 SDK 精确依赖 `=3.3.5`；实际公开版本身份尚未核实 |
| `eltdx-runtime` | `3.3.5`，沿用 workspace 版本线 | SDK 精确依赖 `=3.3.5`，runtime 同时依赖 protocol |
| Python wheel/sdist `eltdx` | `3.3.5`，保持原版本来源 | PyO3 绑定依赖 SDK `=0.1.0` 和两个下层库 `=3.3.5`，构建后封入 wheel |
| `eltdx-python` Cargo 绑定包 | `3.3.5`，`publish=false` | 用于 maturin 构建 Python 产品，不作为 Rust SDK 的生产依赖 |

内部依赖同时写 path 和 version。path 用于开发；归档规范化清单使用注册表版本。首次 SDK 使用 0.x；0.1.x 保留已文档化的稳定入口和行为，破坏兼容的公开改动必须升级 minor 并附迁移说明；隐藏的 `__python_compat` 不是 Rust 稳定接口。Python 产品的签名、默认值、模型、异常、hooks 和安装路径仍按原兼容合同维护，SDK 0.x 的版本变化不授权破坏 Python API。

`eltdx`、`eltdx-protocol`、`eltdx-runtime` 的注册表名称、候选版本是否已被使用和账号发布权尚未查询。注册表 404 只表示该时点查询未找到，不能证明命名权。若下层 `3.3.5` 已发布且内容与候选不同，必须选择新的下层版本并同步精确依赖；不能重新上传同版本或把注册表旧源码当作本地新代码。最终 lock 解析也在统一验证阶段完成，当前 lock 不作为新声明的有效解析证据。

三个 Rust 库各自包含 README 和原 LICENSE 的逐字副本，Cargo include 限定 manifest/lock、许可证、说明、src 和适用 examples。SDK 另包含两个小型测试辅助源码 `tests/support/hq_server.rs` 和 `stress_log.rs`，供 `cfg(test)` 私有预算压力模块在归档中编译；它们不进入生产依赖或运行时。其余仓库测试和大型 fixture 留在仓库；这些测试仍须在最终候选 SHA 上完整运行。归档消费不依赖仓库外部语料。项目原非商业学习和数据研究许可不因 SDK 增加而改变，不填入不符的 MIT/Apache SPDX 标签。

## 可复验的执行入口

`scripts/verification/rust_sdk_packages.py` 要求 Python 3.11+，只作为发布验证开发工具，不进入 Rust 库构建链。每次使用新的输出目录并保存原日志、命令、退出码、工具链、lock 和归档/逐文件 hash。

- `registry-observation`：实际读取 crates.io 响应，保存 HTTP 状态、时点、原始内容/hash 和同版本条目；既不发布，也不承诺名称可用。
- `archives`：复制候选到独立 staging workspace，显式给未发布下层加本地 patch；生成独立 lock、逐 crate list/package，记录 `--no-verify`。解包检查规范化依赖、README/LICENSE/src，再用只依赖 eltdx 的外部 consumer 对全部三个解包目录显式 patch，真实运行最小/default/full 和各域非空用例。该模式是内容与消费模拟，不能标为公开注册表通过。Cargo 若仍不能解析下层，则保留失败，先建立本地 registry；不删版本、不借回源码路径、不跳过失败。
- `registry-dry-run`：先查询并下载两个已发布下层归档，核对注册表 checksum、许可证和候选 src 的全部文件字节；不同或 yanked 立即拒绝，要求版本决策。随后执行普通 package 验证及 publish dry-run。这里仍不上传，且不证明账号命名权或最终注册表消费已经通过。

统一执行时可使用：

```sh
python scripts/verification/rust_sdk_packages.py archives --implementation-complete --output /absolute/new-evidence
python scripts/verification/rust_sdk_packages.py registry-observation --implementation-complete --output /absolute/new-registry-evidence
```

三平台 archive consumer 已写入 `rust-sdk.yml` 的独立 job，未触发。无 Python 的纯 Rust 源码消费者与使用 Python 编排的归档内容验证是两个证据项；后者不能代替前者的干净环境结论。最终还需要发布后移除所有 patch 的 registry consumer，以注册表版本重新生成 lock、执行同一业务流程并记录归档 checksum；本次没有授权或执行实际发布。

wheel 构建与消费 job 分开，消费只用 `--only-binary=:all: --no-index --find-links`，没有匹配 wheel 必须失败。普通 wheel 安装不要求 Rust；源码构建或缺少平台 wheel 时，开发者需要 Rust1.89+ 和 maturin。产物证据必须记录 Python版本、OS/架构、ABI/tag、wheel 文件名/hash、安装 native 文件 hash 及 candidate SHA。额外 MCP/HTTP 服务不是 SDK 默认依赖，需单独安装对应 Python extras。

发布前冻结 source SHA、干净状态、真实 Cargo.lock、feature 集、工具链/镜像 digest、全部产物 hash、平台/兼容/压力证据及剩余限制。任何修复后重新产生候选和受影响证据。发布、tag、push 和合并仍是独立操作，本轮脚本只准备可审阅的产物与 dry-run。
