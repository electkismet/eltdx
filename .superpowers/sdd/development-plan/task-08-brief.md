# T08 — 操作上下文和错误边界

Authoritative plan: `docs/rust-sdk/development-plan.md`, T08. Linked specification is binding: `docs/rust-sdk/public-api.md` §6, `docs/rust-sdk/execution-and-compatibility.md` §§3–4, 6–7, and `docs/rust-sdk/decision-register.md` §3.

## Task text

**文件：** 新增 `src/operation.rs`、`policy/retries.rs`；修改 `error.rs`、`hq/executor.rs`；新增 `tests/operation.rs`。

**接口：** `OperationOptions`、`CancellationToken` 公共；`OperationContext` 和执行 policy 私有；正常 Rust 网络方法最后参数 `&OperationOptions`。`RustPublicExecutionProfile` 使用总预算，`PythonLegacyExecutionProfile` 保留旧计时；两者共用业务循环。

- 在公开方法入口、输入验证之前记录 Instant，deadline 从该入口推导；有界验证、metadata、分页、fallback 共用预算，不重置期限。
- 测试第二页开始前超时、第二次拆批前取消、wire retry与host failover总次数达到上限、取消不影响其他操作。
- 错误记录 kind/phase/operation/command/host/attempt/source；不得把所有错误降为字符串。
- 将 close 与业务 token 分开；已经取消的 token 不能阻止释放。
- 通过 `#[doc(hidden)] pub mod __python_compat` 暴露窄绑定桥接（跨 crate 不能用 `pub(crate)`），内部创建 legacy policy；无 PyO3 依赖、不新增 feature，普通 Rust API 不提供 legacy 切换，此桥接不承诺 SDK 公开稳定性。
- 增加双 profile 对照：两页分别低于单请求上限、合计超过总上限时，Rust 超时，旧/新 Python 成功；预取、补查、空值 fallback 次数及无限页选项按各自契约验证。不得给 Python 暗加累计尝试上限。
- 最终统一验证时运行 `cargo test -p eltdx --test operation --locked`。

T04–T12 的跨 crate 类型与执行顺序按内部实施规格实现；`RequestPermit` 将旧启动后计时与新绝对期限交给共同 runtime 执行，业务循环不自行重置 timeout。

## Binding implementation detail

- New Rust options store `Duration` until entry. Explicit zero timeout is immediately expired; timeout and absolute deadline choose the earlier instant; arithmetic overflow is `InvalidArgument`. Builder defaults stay positive.
- User `CancellationToken` is irreversible, cloneable and owner-free. Owner close cancellation is separate; close and cleanup must work even with an already-cancelled business token.
- Capture entry time before validation. An operation owns one ID/session identity, total absolute deadline for Rust, immutable option snapshot, raw policy, cancellation, shared work/attempt counters and first root cause. Metadata type/installation is integrated in T09/T10; do not implement a second metadata store here.
- Python legacy has no fabricated operation-wide 8s or extra-attempt cap. Each child request starts its existing timeout after runtime startup, through the same raw scheduler as Rust. Domain-specific recovery/page limits are represented explicitly, including `None` for unlimited legacy pages.
- Runtime currently has `SubmissionOptions` with only an absolute deadline and a two-attempt `RetryPolicy`. T08 must implement the language-neutral cross-thread attempt-authorization boundary required by execution spec §3.4, plus a legacy raw entry that preserves existing after-startup timing. Extend shared engine/request/supervisor execution as needed; do not fork the I/O loop and do not simply move retries to a facade loop.
- One attempt covers that connect/send cycle; retries/failover and recovery/fallback first attempts consume the shared Rust extra budget. Normal required first requests/pages/metadata do not. Reserve/start/refund atomically under one small budget/admission terminal gate; a unique non-Clone permit either starts once or refunds an unstarted reservation on Drop. At commit recheck owner/domain close, user cancellation and deadline, not merely at reservation. No locks across I/O, parser, callbacks or waits. Local recovery and global limits reserve transactionally, no partial debit leak.
- Every logical request still has its own finite wire-attempt maximum. Reaching this maximum preserves its last real network error; shared extra-budget exhaustion produces `LimitExceeded` with limit/configured/started and last recoverable source. Retry permission is command/failure-phase aware in new Rust, not blindly the legacy `CommandRequest::retry_safe()` (currently always true). Existing legacy execution must keep its old policy.
- Poll pending work with cancellation/deadline boundaries and perform bounded retirement confirmation once per operation, using cleanup timeout intersected with an active owner shutdown deadline. `CancelUnconfirmed` retains original Timeout/Cancelled cause plus cleanup failure. Never return business success after cancellation wins. Preserve runtime errors as sources and copy structured diagnostics, including phase/command/host/attempt; SDK error must remain language-neutral and must not store PyErr.
- Integrate the owner-bound HQ executor with these contexts and permits; keep build() lazy. No public business APIs exist yet, so expose only the necessary controlled internal execution boundary. Narrow doc-hidden bridge may wrap a legacy executor/context for the upcoming Python binding task; no legacy switch on ordinary client/options.
- Meaningful test SOURCE should exercise the real budget, deadline/profile and owner/cancellation transition code, including concurrent final-extra-slot contention, unused reservation refund, cancelled-before-start refund, owner-close/start race, source preservation, empty/zero-budget legitimate requests and per-request-vs-operation limits. Deterministic injected instants may be used for boundary arithmetic; no execution in this phase.

## Workflow constraints

Working directory: `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`; branch `codex/rust-sdk-implementation`. Read current source; T01–T07 are existing work, do not reimplement them.

User instruction overrides RED/GREEN and per-task verification: DO NOT run tests, collection, cargo build/check/test/clippy/doctest, network probes, compile/AST scripts, diff checks, format checks or equivalent verification until ALL T01–T30 implementation code is complete. Read source and author implementation/test source only. `git status/log/add/commit` and generating a plain review diff artifact are allowed; no verification hooks. No push, tag, publishing or merge. Do not spawn agents; controller handles review.

Write detailed report to `.superpowers/sdd/development-plan/task-08-report.md`: implementation, files, test sources authored, constraints/no execution, self-review and remaining concerns. Commit only owned code and report files in named checkpoints; do not edit `progress.md` or `docs/rust-sdk/IMPLEMENTATION_STATUS.md` (controller owns these). Report under 15 lines with status, SHA(s), no-test statement, concern(s), report path.
