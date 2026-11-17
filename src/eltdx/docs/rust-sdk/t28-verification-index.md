# T28：独立消费者与平台矩阵

下表定位验证入口和证据范围。实际报告见[统一验证记录](unified-validation.md)：Stage131–132 为 Windows 平台与 feature/Loom 验证，Stage134 为 Linux Rust 1.89 full 与 Loom，Stage149 为两个互相独立的工具链镜像；Stage159、166、167、169 分别验证了冻结候选的 macOS Rust 和归档、五平台 Python wheel 安装、Linux Rust 1.100 检查，以及无 Python 镜像中的独立 Rust 消费。每份报告保留实际候选 SHA；公开 registry 消费另见[发布政策](release-policy.md)。

| 要求 | 源码入口 | 执行证据要求 |
| --- | --- | --- |
| 不依赖 Python 的公开 Rust 消费 | `tests/consumers/rust-sdk/`、`scripts/verification/rust_sdk_no_python.sh` | 新建外部目录、独立 lock/target、真实非空 HQ/F10/EX/MAC/MAC-EX 回环及本地文件、重复关闭 |
| 生产依赖闭包 | `rust_sdk_matrix.ps1`、`rust_sdk_no_python.sh` | Cargo 成功后检查 normal/build tree，出现 PyO3、绑定或 maturin 即失败 |
| feature 完整性 | `rust_sdk_matrix.ps1 -Mode features` | 五个 feature 的 32 个显式子集、default、full 别名，不能只验证 full |
| feature 排除 | `rust_sdk_no_python.sh` | 最小基线先成功；禁用模块触发指定 E0432 和模块名；开启对应 feature 再成功，依赖失败不算排除证据 |
| 平台与 MSRV | `rust_sdk_matrix.ps1 -Mode platform`、`rust-sdk.yml` | 1.89.0/stable × Linux/Windows/macOS；最小/default/full、库/示例/doctest/doc/clippy/fmt 和外部消费者 |
| runtime 并发模型 | `rust-sdk.yml` 的独立 loom job | 单列运行和失败日志，不能代替 socket 用例 |
| Python 普通二进制安装 | `consume_python_wheel.py`、`native-wheels.yml` | 构建与消费 job 分离；只装本地 wheel、外部目录导入、native 字节匹配，Python3.10/3.14 × 五个平台 wheel |

Rust 消费者只有一个直接依赖 `eltdx`，内部 wire fixture 只用 std。扩展源码核对独立字面量请求、字段和 raw，显式关闭后必须观测 EOF。MAC-EX 只在该合成 fixture 显式配置合成登录规则，不补造生产默认值。独立 consumer 的源码存在不能证明 `.crate` 内容完整；T29 的归档消费者另列。

无 Python 的 Linux job 使用 Rust-only slim 容器，并在复制/编译前拒绝 `python/python3/pip/pip3/maturin` 可执行入口。Python-only Linux 消费 job 使用独立 Python slim 镜像，在创建 venv 前拒绝 Rust/maturin。Windows/macOS hosted job 证明消费进程拿不到构建工具；报告明确为 `process-path-only`，不能升级为整台机器未安装编译器。更强的桌面隔离证据需最终执行时提供专用干净镜像。

脚本保存候选 SHA、工作区修改、工具链、系统/架构、lock SHA256、每条命令的参数/退出码/耗时及日志 hash。每次使用新证据目录，失败不得覆写为通过。Rust-only shell 的日志、退出码和 tree 单独保留；执行器还须记录镜像 digest 和调用时的候选 SHA，标签本身不是不可变镜像身份。

全源码完成后已解析并锁定根 Cargo.lock，再按各报告所列 SHA 运行 `--locked` 门禁。离线行为测试使用本地 fixture；真实市场采样、压力和性能分别记录。Stage159、166 的 GitHub Actions 已生成可核验产物；分支验证不创建 tag 或发布包。
