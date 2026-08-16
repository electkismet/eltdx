# ADR-0001：扩展 Native Engine 到160条以上 TCP 连接

**状态**：Accepted
**日期**：2026-08-16

## 背景

本次决策前的3.0候选中，每个 Engine 使用一个 current-thread Tokio runtime。它比2.0.5的每 Slot 一个 Python Actor 线程更容易控制生命周期，但在160条以上 TCP 连接和大量解压、推送同时发生时，CPU 工作只能落在一个 runtime 线程上。同时，每 Slot 8 MiB decoded queue 的局部上限在160个 Slot 下形成1.28 GiB理论总量。

服务器选择也需要从包内43台候选服务器中按当前机器网络实测排名，且排名跨进程保留。

## 决策驱动

- 支持160条及更多独立 TCP 连接。
- 强机器能够自动使用更多逻辑处理器，也允许手动设置。
- 保持 Supervisor 单所有者和 generation 隔离合同。
- 线程数不能随 TCP 连接数一比一增长。
- 内存使用必须有 Engine 级硬上限。
- 服务器排名必须持久化，升级软件不能覆盖用户本地结果。

## 考虑的方案

1. 每个 Slot 一个线程：隔离直观，但160个 Slot 就接近160个线程，栈内存和调度成本过高。
2. 多个独立 Engine：可以利用多核，但公开生命周期、pin、push、FIFO和全局容量会被拆散，用户还要自行管理分片。
3. 单 Engine、单 Supervisor、多线程 Tokio runtime：异步 Slot 共享有限数量 worker，同时保留一个状态所有者。

## 决策

采用方案3：一个逻辑 Engine、一个单所有者 Supervisor、一个多线程 Tokio runtime，以及任意配置数量的异步 Slot。

- 默认选择43台测速排名中最快的2台，每台4条连接。
- `runtime_workers` 默认取 `min(pool_size, available_parallelism)`，允许显式覆盖。
- 排名写入用户数据目录的 `tdx_server_ranking.json`；包内 `tdx_server.json` 只保存候选名单。
- 连接先按服务器做 canary 握手，再有界分批扩容。
- 保留单 Slot 8 MiB decoded 边界，并增加随 Slot 数增长的 Engine 全局 raw、decoded 和 push 字节预算；160个 Slot默认允许1.28 GiB decoded 总量。
- `pool_size` 继续表示总 Slot 数，不改成线程数。

## 后果

正面结果：

- 160条连接不需要160个线程。
- 解压和网络事件能够使用多个逻辑处理器。
- 服务器连接分布、总内存和启动并发均可配置且有界。
- 保留当前3.0最重要的请求归属和关闭正确性。

负面结果：

- 多线程 runtime 的状态发布、预算归还和关闭证明更复杂。
- 强机器的最佳 worker、建连并发和内存预算仍需统一压力测试确定。
- 自动全量测速会增加第一次真正连接前的等待时间。

## 风险控制

- Supervisor 仍是共享池状态的唯一写入者。
- worker 不直接改变 lease、pin、terminal owner 或 Engine 生命周期。
- 所有容量使用 permit 或等价的原子预算转移，不能重复计数。
- 任何 Slot 局部超限只 retirement 当前 generation，不直接失败整个 Engine。
- 完整实现方案见 [线程层改造方案](../THREADING_SCALE_PLAN.md)。
