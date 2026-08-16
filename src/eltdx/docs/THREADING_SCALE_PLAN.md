# eltdx 160+ TCP 连接线程层改造方案

> 状态：本机统一验证完成，等待跨平台 CI
> 日期：2026-08-16
> 范围：7709 Native Engine、服务器选择、连接池、内存和取消路径

本文记录3.0线程层扩容的实现合同。43台服务器测速排名持久化、多线程 Tokio runtime、服务器连接分布、受控建连和全局内存预算已写入候选代码。本机编译、协议差分、公开接口、loopback、压力和真实主站验证已经通过；Windows、Linux 和五平台 wheel 仍以候选提交的 CI 结果为准。

## 目标

- 默认配置简单：从43台服务器中选最快的2台，每台4条 TCP 连接，共8个 Slot。
- 支持20台服务器乘以每台8条连接，即160个 Slot，并能继续向上配置。
- TCP 连接数和 CPU 工作线程数彻底分开，160条连接不等于160个线程。
- 强机器自动使用更多逻辑处理器，用户也可以手动指定工作线程数。
- 任何队列和缓冲区都有字节或数量上限，连接增加时内存不能按最坏情况无限增长。
- 保持一个请求只能由一个 terminal owner 完成、一个 socket 同时只有一个业务请求在途、迟到响应不能污染新请求等现有正确性合同。

## 名词

| 名称 | 大白话含义 |
| --- | --- |
| `server_count` | 从测速排名中使用几台服务器，默认2 |
| `connections_per_server` | 每台服务器建立几条 TCP 连接，也就是几个 Slot，默认4 |
| `pool_size` | Engine 的 TCP 连接总数，默认由前两项计算为8 |
| `runtime_workers` | 真正执行网络事件、解压和协议处理的 CPU 工作线程数 |
| Slot | 一条独立 TCP 连接及其 decoder、generation 和当前请求状态 |
| Supervisor | 全池唯一的调度和状态所有者，不直接做重解压或 socket I/O |

“每台服务器4线程”不是准确说法，应写成“每台服务器4条 TCP 连接”或“4个 Slot”。工作线程由整个 Engine 共享，不属于某一台服务器。

## 服务器测速和持久排名

### 候选名单

安装包内的 `src/eltdx/tdx_server.json` 保存43台官方候选服务器。它是候选来源，不保存某一台电脑的网络结果。

每个用户另有可写的 `tdx_server_ranking.json`：

- Windows：用户应用数据目录下的 `eltdx/tdx_server_ranking.json`
- macOS：`~/Library/Application Support/eltdx/tdx_server_ranking.json`
- Linux：`$XDG_DATA_HOME/eltdx/tdx_server_ranking.json`，未设置时使用 `~/.local/share/eltdx/`
- 可用 `ELTDX_DATA_DIR` 改变目录

排名表记录服务器顺序、最近延迟、最近测速时间、最近成功时间和连续失败次数。写入必须先写临时文件，再原子替换，程序中断不能留下半个 JSON。

### 默认流程

```text
读取43台候选服务器
  -> 读取上次持久排名，立即得到可用顺序
  -> 第一次真正建立 Engine 前并发测速全部43台
  -> 原子更新本地排名表
  -> 从可达服务器中选最快的前 server_count 台
  -> 每台先做1条 canary 协议握手
  -> 握手成功后分批建立剩余连接
```

测速阶段测 TCP connect 延迟；真正扩容前由 Rust canary 完成7709握手校验，避免把“端口很快但协议不可用”当作可用连接。失败 Slot 按持久排名优先使用未占满的后备服务器。

用户手动调用 `refresh_server_ranking()` 时同样测速并持久化。一次测速全部失败时，不能清空上一次排名；失败状态可以更新，但旧的有效顺序和最近成功信息必须保留。当前服务器失效时，按持久排名使用下一台候选服务器补位。

## 当前候选运行结构

```text
Python 同步调用线程
        |
        v
有界 admission + 25ms signal polling
        |
        v
一个 Native Engine
        |
        +-- 一个单所有者 Supervisor
        |
        +-- Tokio runtime（1 worker 为 current-thread，2+ 为 multi-thread）
        |       +-- worker 1
        |       +-- worker 2
        |       +-- ...
        |
        +-- 8、160或更多异步 Slot
                +-- 每个 Slot 独占一条 TcpStream
                +-- 每个 Slot 独立 generation 和 decoder
                +-- 每个 Slot 最多一个业务请求在途
```

Supervisor 继续是 epoch、FIFO、lease、pin、push、terminal owner 和 diagnostics 的唯一写入者。Tokio worker 可以并行执行 Slot I/O、解压和协议处理，但不能绕过 Supervisor 直接修改全池状态。

## 工作线程数量

自动值：

```text
runtime_workers = min(pool_size, 当前系统可用逻辑处理器数量)
```

Rust 使用 `std::thread::available_parallelism()` 读取 Windows、macOS 和 Linux 当前允许本进程使用的逻辑处理器数量。它比读取 CPU 型号更准确，因为容器和进程 CPU 配额可能小于机器总数。

- 8个 Slot、16个逻辑处理器：默认8个 worker。
- 160个 Slot、16个逻辑处理器：默认16个 worker。
- 160个 Slot、64个逻辑处理器：默认64个 worker。
- 用户显式设置 `runtime_workers` 时使用用户值，但必须在1到 `pool_size` 之间；worker 多于 Slot 不会增加并行能力，只会浪费线程资源。
- `runtime_workers=1` 使用 current-thread 快速路径，避免单连接场景承担多线程调度开销；两个以上 worker 才启用 multi-thread runtime。

worker 数量决定同一时刻能有多少 CPU 工作并行执行，不限制等待网络的连接数量。160个异步 Slot 即使只有16个 worker，也仍然可以保持160条 TCP 连接并发等待数据。

## 服务器和连接配置

公开配置：

```python
TdxClient(
    server_count=2,
    connections_per_server=4,
    runtime_workers=None,
)
```

高并发示例：

```python
TdxClient(
    server_count=20,
    connections_per_server=8,
    runtime_workers=None,
)
```

兼容规则：

- 新代码优先使用 `server_count` 和 `connections_per_server`。
- `pool_size` 继续表示总 Slot 数，不能改变成线程数。
- 未显式传 `pool_size` 时，`pool_size = server_count * connections_per_server`。
- 只显式传旧参数 `pool_size=N` 时，在选中的最快服务器之间尽量平均分配 N 个 Slot。
- 同时显式传三项时，`pool_size` 必须等于前两项乘积，否则立即抛 `ValueError`，不能悄悄改写用户配置。
- `max_connections_per_host` 可作为高级安全上限；默认等于 `connections_per_server`，允许用户手动提高。

## 分批建立连接

160条连接不能同时发起连接和握手。候选实现过程：

1. 对选中的每台服务器先建立1条 canary 连接并完成真实握手。
2. 握手失败立即换排名中的下一台，不为失败服务器继续扩容。
3. canary 全部成功后，再分批建立剩余 Slot。
4. 默认全局建连并发：

```text
min(pool_size, max(4, min(可用逻辑处理器数量 * 2, 32)))
```

5. 默认每台服务器同时最多建立2条连接，避免短时间冲击同一主站。
6. `connect_concurrency` 和 `connect_concurrency_per_host` 都允许手动设置。强机器可以提高，但不能使用无上限值。
7. 后备服务器已达到 `max_connections_per_host` 时立即尝试下一台，不能占着请求 deadline 等一个不会释放的活动连接名额。
8. 单个 Slot 即使持有43台候选，连接公平切片的分母也最多按8台计算；默认8秒超时下第一台至少获得约1秒，后续候选仍共享剩余总 deadline。

这两个默认公式是候选安全值；160条及更高连接数的统一压力测试尚未运行，测试结果可能要求调整默认值。

## 公平调度

保留当前每个 Slot 单轮预算：

- 最多读取256 KiB wire 数据
- 最多路由64帧
- 最多产生4 MiB decoded 数据
- 达到任一预算后主动 yield，让其他 Slot 获得执行机会

这些数字不是总速度限制。一个 Slot 下一轮可以继续处理；它们只防止单个大包或推送洪峰长期占住 worker。

## 全局内存预算

单个7709响应帧的 decoded 长度最大约64 KiB。当前每个 Slot 每轮最多生成4 MiB decoded 数据，队列最多保留1024帧且8 MiB，因此8 MiB相当于约两轮处理余量，继续作为默认单 Slot 上限。提高到16 MiB通常只会延迟过载暴露，不会提高实际处理速度。

Engine 全局预算随 Slot 数自动增长，不能再固定为256 MiB，否则160个 Slot会过早背压。

默认值：

| 预算 | 默认上限 | 说明 |
| --- | ---: | --- |
| 单 Slot raw staging | 约320 KiB现有协议硬界 | 防止半包和异常长度无限增长 |
| 单 Slot decoded queue | 1024帧且8 MiB | 可容纳约128个最大尺寸帧 |
| Engine 全局 raw | `min(pool_size × 单 Slot raw 上限, 256 MiB)` | 160个 Slot约51 MiB |
| Engine 全局 decoded | `min(pool_size × 8 MiB, 2 GiB)` | 160个 Slot允许完整使用1.28 GiB |
| Engine 全局 push | 64 MiB | 用户尚未读取的推送总和 |

这些是最大允许值，不会在启动时预分配。`global_raw_bytes=None` 和 `global_decoded_bytes=None` 表示使用自动值；高配置机器可以手动提高全局预算，但不提供“无限”选项。单 Slot decoded 保持8 MiB硬上限，不能绕过全局预算。

容量不足时按以下顺序处理：

1. 优先保证已发出业务请求的响应。
2. 暂停或减慢受影响 Slot 的读取，让 TCP 自身产生背压。
3. push 满时丢最旧 push，并通过 `PushOverflowError` 和 diagnostics 告知用户。
4. 单 Slot decoder 自身超限时只废弃该 Slot generation，使受影响请求失败；其他 Slot 和 Engine 继续运行。
5. 不因一个 Slot 超限直接关闭整个 Engine，也不允许继续无界分配内存。

## Ctrl+C、diagnostics 和关闭

- Python 提交请求时使用非阻塞 admission；成功入队后立即进入25ms一次的 signal polling。
- admission 已满时立即抛 `PoolBusyError`，不能在持有 GIL 时无限等 runtime 腾位置。
- diagnostics 读取 Supervisor 发布的缓存 snapshot，不同步等待 runtime 回答。
- 完整 diagnostics 缓存最多每25 ms刷新一次，不能在每个请求事件后遍历全部 Slot。
- cancel、close 和 fatal 使用独立的高优先级控制路径，普通请求队列满时仍能执行。
- 已发送请求取消或超时后必须废弃对应 TCP generation，迟到响应不能完成后续请求。
- `close()`、`FAILED_CLOSING -> FAILED_CLOSED`、不可 reopen 和 terminal 单所有者规则保持现有3.0合同。

## 与2.0.5和当前3.0的对比

| 方案 | 160条连接时的线程形态 | 优点 | 主要缺点 |
| --- | --- | --- | --- |
| 2.0.5 Python Actor | 接近每个 Slot 一个 OS 线程 | 模型直观，连接间天然分散 | 线程和栈内存随连接数增长，GIL和上下文切换成本高 |
| 改造前3.0候选 | 一个 Engine 一个 current-thread runtime | 状态简单，线程少，正确性边界清楚 | CPU 密集解压和推送洪峰受单个 runtime 线程上限约束 |
| 当前候选 | 一个 Engine，共享自动数量的 worker，160个异步 Slot | 保留单 Supervisor 正确性，同时使用多核并控制线程数 | 调度、全局预算和关闭证明更复杂，必须做高强度压力验证 |

目标方案借鉴2.0.5“不同连接互不共享 socket”的隔离优点，但不恢复“每条连接一个线程”。

## 实施和验证顺序

1. 已完成：冻结新配置语义、兼容规则和 diagnostics 字段。
2. 已完成：将 Native Engine 改为自适应 Tokio runtime，同时保留单 Supervisor 状态所有权；1 worker 走 current-thread 快速路径，2个以上 worker 走 multi-thread。
3. 已完成：实现服务器排名选择、真实握手验证、连接均匀分布和失败补位。
4. 已完成：实现 canary 与有界分批建连。
5. 已完成：实现 Engine 全局 raw、decoded 和 push 预算及背压。
6. 已完成：将 admission 改为非阻塞提交，将 diagnostics 改为缓存快照。
7. 已完成：完成所有代码、合同和测试代码的静态审查后冻结实现。
8. 待执行：按用户要求，代码全部写完之前不运行测试；统一测试阶段再首次编译并从头执行完整验证。

不得在完成统一测试、审查结果和用户确认之前提交发版。
