# Rust SDK 与 Python 版本、打包及发布政策

eltdx 每次用同一个 Git 标签发布 Rust 与 Python 包。`v3.4.0-beta.1` 对应 Cargo 的 `3.4.0-beta.1` 和 PyPI 的 `3.4.0b1`；正式版本号直接一致。原 `0.1.0` Rust 源码测试版和 `3.3.5` wheel 候选的历史记录见[统一验证](unified-validation.md)。

| 产品或组件 | 候选版本 | 依赖关系与安装 |
| --- | --- | --- |
| Rust 业务 SDK `eltdx` | `3.4.0-beta.1` | 用户通过 Cargo 消费；不依赖 Python |
| `eltdx-protocol` / `eltdx-runtime` | `3.4.0-beta.1` | Cargo 自动解析 SDK 精确依赖的下层包 |
| Python wheel/sdist `eltdx` | `3.4.0b1` | 已编译的共享 Rust 核心封入 wheel |
| `eltdx-python` Cargo 绑定包 | `3.4.0-beta.1`，`publish=false` | 仅用于 maturin 构建 Python 包 |

内部依赖同时写 path 和精确 version：path 用于开发，归档使用注册表版本。主项目版本由 Cargo workspace 管理；升版同步更新精确依赖和锁文件。测试版是用户主动选装的版本。Python 的公开用法按原兼容合同维护，`__python_compat` 是绑定实现细节。

`eltdx`、`eltdx-protocol`、`eltdx-runtime` 于2026年11月17日11:43:52–53 UTC的实际crates.io查询均返回404，原响应/hash见stage126的registry-observation-attempt01；账号发布权尚未验证。注册表404只表示该时点查询未找到，不能证明命名权。若下层 `3.3.5` 已发布且内容与候选不同，必须选择新的下层版本并同步精确依赖；不能重新上传同版本或把注册表旧源码当作本地新代码。实际lock已于统一验证阶段解析并使用Rust1.89执行；lock及源码身份仍必须与每次验收候选对应。

stage137在提交`aa4255a2e9baeb547ae70f30a67b671bef5cef99`上再次进行只读观察，`eltdx-protocol@3.3.5`、`eltdx-runtime@3.3.5`和`eltdx@0.1.0`均返回HTTP404，时间为`2026-11-17T17:55:41Z`。响应原文与SHA256、候选lock和工具版本见`artifacts/unified/stage137-registry-observation-attempt01/packages.json`（SHA256 `4cb9969e56817b5149ffeb9f5e1dbf9229d6dc9f068c0e2c4fa073692c360f91`）。这只是该时点的名称存在性观察，不代表保留名称、账号权限、发布成功或注册表消费。

stage138的Windows/Rust1.89归档验证以同一当前源码树执行，24条工具命令全部exit0，三个归档的manifest/文件内容已检查；从实际解包文件运行11种最小/default/full consumer模式均成功。结果见`artifacts/unified/stage138-archives-attempt01/packages.json`（SHA256 `1519ace848c8809e5a8ad8c337c9a043955f9c26a444c2683471a47f38e1530f`）。下层包由本地目录源登记，消费者显式patch到真实解包内容；不属于公开registry身份或发布验证。

三个 Rust 库各自包含 README 和原 LICENSE 的逐字副本，Cargo include 限定 manifest/lock、许可证、说明、src 和适用 examples。SDK 另包含两个小型测试辅助源码 `tests/support/hq_server.rs` 和 `stress_log.rs`，供 `cfg(test)` 私有预算压力模块在归档中编译；它们不进入生产依赖或运行时。其余仓库测试和大型 fixture 留在仓库；这些测试仍须在最终候选 SHA 上完整运行。归档消费不依赖仓库外部语料。项目原非商业学习和数据研究许可不因 SDK 增加而改变，不填入不符的 MIT/Apache SPDX 标签。

## 可复验的执行入口

`scripts/verification/rust_sdk_packages.py` 要求 Python 3.11+，只作为发布验证开发工具，不进入 Rust 库构建链。每次使用新的输出目录并保存原日志、命令、退出码、工具链、lock 和归档/逐文件 hash。

- `registry-observation`：实际读取 crates.io 响应，保存 HTTP 状态、时点、原始内容/hash 和同版本条目；既不发布，也不承诺名称可用。
- `archives`：复制候选到独立staging workspace，vendor真实locked依赖，并只在staging/consumer配置中替换为本地目录源。按依赖顺序逐crate list/package（记录`--no-verify`），安全解包检查规范化依赖、README/LICENSE/src；以实际归档及逐文件checksum登记目录源，令后续包能解析未发布下层。外部consumer只依赖eltdx，对全部三个解包目录显式patch，真实运行最小/default/full和各域非空用例。目录替换及patch均不进入产品manifest，也不等于公开注册表通过；不删版本、不借回工作区源码、不跳过失败。stage126的原workspace-patch失败和目录源成功探针均保留，正式流水线仍须实际执行。
- `registry-dry-run`：先查询并下载两个已发布下层归档，核对注册表 checksum、许可证和候选 src 的全部文件字节；不同或 yanked 立即拒绝，要求版本决策。随后执行普通 package 验证及 publish dry-run。这里仍不上传，且不证明账号命名权或最终注册表消费已经通过。
- `registry-consumer`：三包都可从 crates.io 下载后，核对公开 checksum、候选源码与许可证。随后在仓库外用全新 Cargo home、不继承 Cargo 配置或缓存，仅依赖 SDK 精确版本生成 lock；核对三包版本、来源与 checksum，运行 11 种消费者场景。缺少包或身份不符即失败并保留证据。

统一执行时可使用：

```sh
python scripts/verification/rust_sdk_packages.py archives --implementation-complete --output /absolute/new-evidence
python scripts/verification/rust_sdk_packages.py registry-observation --implementation-complete --output /absolute/new-registry-evidence
python scripts/verification/rust_sdk_packages.py registry-consumer --implementation-complete --output /absolute/new-consumer-evidence
```

`Publish` 工作流支持从候选分支手动演练，不上传产物到公开注册表。推送与 workspace 版本匹配的 `v` 标签后，同一次运行构建并验证 Python wheel/sdist 和 Rust 归档，按依赖顺序发布 Rust 包，验证公开索引消费，再通过 PyPI Trusted Publishing 上传 Python 包。两端成功后才创建 GitHub Release；预发布标签不会成为项目的最新正式版。性能、生产抓包与外部线路验证另行记录。

Stage152 的 Windows/Linux 归档比对确认，换行会改变许可证与原始清单的字节，使用本地目录源打包时还会改变上层包锁文件中下层归档的 checksum。正式发布使用一份固定 SHA、固定换行设置的干净检出，并从同一份检出运行发布前后字节校验。公开 registry 的 checksum 必须来自实际发布的归档，不得从本地模拟产物推定，不放宽已发布源码与许可证的逐字节身份校验。实测见[统一验证记录](unified-validation.md) Stage152。

## 首发与中断后续跑 { #initial-registry-release }

1. 在 GitHub Actions secrets 设置获授权的 `CARGO_REGISTRY_TOKEN`，首次发布需允许创建三个 crate。令牌不写入命令参数、文件或聊天。检查版本能否发布，404 只表示该时点未找到。
2. 工作流上的 `registry-publish` 依次处理 `eltdx-protocol` → `eltdx-runtime` → `eltdx`。确认版本未撤回、归档散列与索引相同、源码和许可证与发布提交一致后，再继续。全部上架后用独立 Cargo 项目消费公开版本。
3. 如果上传中断，先确认索引是否已经出现，再重跑同一个标签的失败工作流。已经存在的同版 Rust 归档只有在来源提交和源码一致时才会跳过；同版归档冲突停止发布，由维护者处理。两个注册表不能原子上传，所以 PyPI 失败时 Rust 包可能已经可用，GitHub 统一发布页会等 PyPI 完成。

wheel 构建与消费 job 分开，消费只用 `--only-binary=:all: --no-index --find-links`，没有匹配 wheel 必须失败。普通 wheel 安装不要求 Rust；源码构建或缺少平台 wheel 时，开发者需要 Rust1.89+ 和 maturin。产物证据必须记录 Python版本、OS/架构、ABI/tag、wheel 文件名/hash、安装 native 文件 hash 及 candidate SHA。额外 MCP/HTTP 服务不是 SDK 默认依赖，需单独安装对应 Python extras。

测试版可用候选分支提交发布；稳定版从合并到 `main` 的提交发布。每次保留标签、产物散列和验证记录，修复后使用新的测试版号。
