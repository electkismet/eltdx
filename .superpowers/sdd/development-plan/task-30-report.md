# T30 source implementation report

## First checkpoint — native stress sources and evidence reader (stage111)

Base commit43ec6b5. T30 is **partial**, not source-complete or accepted. Current totals:28 source-complete /2 partial(T19/T30) /0 unstarted. The full unlimited goal remains active. No tests, collection, compile/build/check/clippy/doctests, dependency/network, AST/format/diff checks or equivalent execution ran. No subagents, publication, push, tag or merge.

Authored ten explicitly ignored cases across tests/stress.rs, tests/stress_mac.rs and the private cfg(test) src/stress_internal.rs. Full workload/source/evidence/threshold mapping lives in docs/rust-sdk/t30-evidence-index.md. Actual paths include100k wire requests among125k unique outcomes, admitted cancellation/late replies, close/pin/service handles, paused subscription overflow, malformed replies/pagination/download limits, reconnect/retry, real DAT/ZIP caps, F10 HTTP owner workers, MAC-EX catalog/macro cache ownership and actual SDK engine raw/decoded/queue/actor/generation telemetry. None has run.

The HQ loopback helper now assigns physical identities, counts live connections and reaps child JoinHandles during long workloads. Its first error is retained while all children are stopped/joined. Final ordinary regression must cover this shared helper change. SDK archive include adds its two small test helpers because the private library-test module uses them; release-policy.md explains this, production imports unchanged.

New scripts/verification/rust_sdk_stress.py builds before measuring, locates real release test binaries, runs each ignored case in its own process, observes process-tree RSS/threads/FD or handles/TCP/CPU with psutil, retains stdout/events/resource NDJSON and source/blob/lock/toolchain/command/exit/hash provenance. verify/aggregate re-open raw evidence rather than trusting a status field, validate identities/counts/budgets/resources/percentiles and require same clean candidate Linux+Windows. Independent manual rust-sdk-stress.yml preserves failed evidence; ordinary jobs do not execute load. All runner/workflow/dependency APIs are unverified.

Source-read corrections in this checkpoint: fixed cancellation result unwrapping; release fixture before propagating caller error; infinite directory must return LimitExceeded; actually exercise malformed company catalog; connect logical operation ID to physical wire identity; discard stale activation notifications; fixture errors reap all children. These are source edits, not verified fixes.

Remaining T30 source scope: complete performance workloads/runner/exports and old/new same-fixture policy, fuller workload readiness/warmup/configuration and close/cancel timing, final R01–R12 evidence index, rustdoc/examples/Python migration/release/maintenance, and focused source self-review. T19 still needs source completion before global execution gate. Production login/setup evidence remains absent; synthetic rules do not fill it. Do not run any new script yet.

Next recovery: read this report and t30-evidence-index.md, inspect current Git and code, continue remaining T30/T19 implementation. Do not mistake28 source-complete tasks for28 accepted tasks. Canonical docs and package mirrors must stay synchronized. Next checkpoint stage112, not another stage111.

## Second checkpoint — paired performance and workload readiness (stage112)

Base c5584c9. Counts remain28 source-complete /2 partial(T19/T30) /0 unstarted. No compilation, tests, collection, dependency/network, syntax/AST/format/diff checks or equivalent execution ran. Only source/document reads/writes and ordinary Git recovery operations. No agents, push/tag/publish/merge, or goal-controller changes.

Authored measure_candidate.rs using public SDK/std with offline feature: strict in-memory DAT parse, reused HQ loopback, actual connected-owner close, separately named pre-admission cancellation and optional explicit external observation. Python worker imports only installed wheel public APIs and records actual native binary hash. Separate parse and predecoded FinancialRecord construction curves do not infer conversion time by subtracting incompatible Rust/legacy profiles.

performance_fixture.py independently creates256x64 float32 DAT and a nonempty standard-library HQ server, with strict14-byte market/start/limit/reserved request validation, monotonic physical connection IDs and full observed traffic. rust_sdk_performance.py builds actual baseline/candidate release wheels with locked dependency inputs/profile/toolchain provenance, installs separately with no-index/binary-only, builds/copies the release Rust worker, alternates12 curves across7 trials with50 warmup+500 samples each, and samples only worker resources. Raw samples/wire/peer failures/logs, build artifacts and original records/lock/source/script/binary hashes are retained. verify re-reads raw evidence and checks snapshots against candidate Git blob contents. All84 default runs remain unexecuted.

Manual rust-sdk-performance.yml separates preparation/measurement/artifact export from quick CI. performance-policy.md documents exact scopes, source/build commands, CPU/RSS sampling limits, uncontrollable external observations and candidate/baseline ratio direction. It makes no speedup or passing claim.

All ten stress paths now author actual warmup, one configuration and final measurement/end/cooldown boundaries. Separate latency observations cover actual admitted cancellation and connected owner close, with raw identity/count rechecking. Fixed malformed company command to0x02cf after reading its production command source and subscription SecurityId construction. Source-only self-review corrected an initial6-byte directory fixture assumption to the actual14-byte request, resource hashes calculated before buffered file close, failed peer records discarded before summary, cleanup after failed close-preparation queries, and source snapshot hashes not tied to actual Git bytes. None is called a verified runtime fix.

Next stage113: finish focused pressure/performance source review (including generation/resource evidence), finalR01–R12 coverage handoff, crate rustdoc/example/feature/error/unit/time/raw/serde explanations, Python migration/release/maintenance documents and strict/link verifier sources. Then close T19 focused source inventory/review/handoff. Only after ALLT01–T30 source scopes are actually complete may unified execution begin. ProductionMAC-EX login and optionalMAC setup identity/order/fence evidence remain absent; synthetic ACKs never fill them. Keep canonical documentation and src/eltdx/docs/rust-sdk mirrors synchronized.

## Third checkpoint — final guides, transport handoff and strict docs (stage113)

Base58f7664. The actual source additions close T19's implementation/fixture/
handoff scope. Current totals29 source-complete /1 partial(T30) /0 unstarted.
No source gate has been opened and no execution verification ran. The unlimited
goal remains active; no agents, publication or goal-controller changes.

Added user-guide.md, python-migration.md, final-acceptance.md, release-notes.md,
maintenance.md and f10-transport-index.md. They explain independent installs,
features, ownership, deadlines/errors/cleanup, ordering/completion, units/time/
raw/serde, preserved Python adapters, current candidate scope and unresolved
production evidence. R01–R12 link real domain indexes, implementations, actual
test targets and required run provenance; the table is navigation, not an
acceptance program that passes merely because files exist.

Crate homepage rustdoc has a non-network lazy-owner doctest and a no_run actual
codes query with explicit endpoint and primary-versus-close error handling.
EX and financial download examples now also report secondary close failure.
The README documents the gated measure_candidate worker. MkDocs has user and
domain guide navigation, and its SDK index now distinguishes original design
baseline from the unverified implementation candidate. The stale status table
is brought forward without altering historical checkpoint records.

rust_sdk_matrix.ps1 authors minimal/default/full doctest and doc runs with
strict RUSTDOCFLAGS, logs the actual flags and restores the original process
environment in finally. The manual SDK workflow adds a separate documentation
job using existing metadata/full mirror, MkDocs strict and built local-link/
fragment checkers; it does not install/build the Python product to build docs.
None of these commands or jobs has run.

Pressure source adds actual actor_slots, live/slot/pool bounds, required live
generation observation and per-slot monotonic epoch/generation. Close requires
zero raw/decoded/live actor/queue; the private test also asserts every remaining
actor is stopped with no pending work. The verifier still independently checks
wire/terminal IDs and inflight late-response isolation. These are authored
conditions, not observed performance or lifecycle results.

Remaining T30 source work is explicit: verification.md requires every public
service to have an ordinary compileable usage example and a nonempty local
behavior case. Homepage examples and general integration inventory alone do not
satisfy this per-service documentation condition. Author the missing examples,
tie each to existing real loopback functions, then perform final source review
of T01–T30 deliverables and synchronize mirrors. Only then may the root record
the global source gate and start unified dependency/build/test/docs execution.
All dependency/API/MSRV/license/platform, package/compatibility, resource and
performance outcomes remain unverified. MAC-EX production login and optional
MAC setup identity/order/fence evidence remain missing.

## Fourth checkpoint — service examples and E06 serialization contracts (stage114)

Basefd548ca,2026-11-17. Completed the remaining authored T30 deliverables:
six canonical example guides copied byte-for-byte into crate src/guide and
feature-gated include_str rustdoc; service-examples.md maps18 public services,
four standalone clients and local financial reading to actual nonempty fixture
functions. Sixteen HQ snippets include the public pin query; other guides use
actual checked F10/financial/EX/MAC/MAC-EX APIs. Examples borrow configured
owners and explain explicit outer cleanup, original units/dates/raw/completion,
per-public-call relative deadlines and the synthetic/production profile limit.

Authored a new nonempty public pinned refresh fixture preserving request ID,
source cursor, raw payload/frame and explicit pin/owner close. Authored seven
serde_contract functions covering representation/scale/extreme i64/provenance,
f32/f64 negative zero, three nonfinite forms per width with unmodified raw bits,
display/source units, JSON byte sequences and absent-vs-empty wire, all five
time representations, checked constructors and explicit Unknown-vs-foreign
variants. Serde guide adds one generic serializer usage and separate compile-
fail contracts for SecurityId/TradingDate/EventTime Deserialize absence. The
JSON dependency for these tests is dev-only; production serde selection remains
independent. Tests/doc examples have NOT been compiled, collected or executed.

Extended the existing metadata checker to six include byte pairs; it proves
only synchronization. Matrix now authors standalone serde integration plus
minimal/default/serde/f10/full strict doctest/doc. Updated actual navigation,
user/maintenance/final acceptance/verification indexes and package mirrors.
These are real source edits, not another file-existence acceptance program.

Final focused source handoff reviewed T30 brief/plan, R01-R12 final index,
service APIs and samples, T28/T29 sources/reports, feature/dependency boundaries,
E01-E10 decisions and existing T19/pressure/performance handoffs. No remaining
promised implementation source gap was identified; missing independent external
evidence stays explicit rather than being invented or replaced by synthetic
fixtures. Source search found no todo!/unimplemented! body in SDK/bindings or
consumers; that search is not proof of correctness or completed test coverage.

Current count30 source-complete/0 partial/0 unstarted. ALL execution acceptance
is still pending; no test/collection/build/check/clippy/doctest, dependency or
network probe, compiler/AST/format/diff-check/equivalent command ran. Ordinary
source/document/Git operations only. Existing historical reports keep their
then-current counts. No subagents/push/tag/publish/merge/goal-controller edits.

Next root action: commit this source checkpoint with hooks disabled, then record
the global source-complete gate and begin unified verification in the order in
final-acceptance.md. Discover actual tools, resolve/lock dependencies, retain raw
failures, fix without reducing feature/API/test scope, and checkpoint repairs.
Uncompiled dependencies/API/MSRV/licenses/platforms, package/compatibility/
resource/performance outcomes and production profiles remain unverified. The
unlimited goal remains active until requirement-by-requirement actual acceptance.

## Global source gate — root checkpoint (stage115)

Root saved source checkpoint23bf1c83718e83d20e9c5ba9b919e9dfac3e2db4 and
observed a clean Git worktree. All authored T01-T30 source scopes are complete;
under the user's instruction, unified execution can now begin. No compile/test/
dependency/network/format/document result exists at the time of this record.
See docs/rust-sdk/unified-validation.md for subsequent actual run outcomes.
Begin tool/lock preparation then correctness/compatibility/package/docs and
pressure/performance; do not reduce scope or invent missing external evidence.
Goal remains active without a token quota; no publishing or goal-state edits.

## Unified validation stage116 — first real tooling and compilation

After gateb94d762, actual Windows Rust1.89/Python3.12 tooling and dependency
resolution ran. Initial lock/Loom MSRV failed at generator0.8.12/Windows0.100
(requires1.95); resolver fallback regenerated a compatible208-package lock and
Loom check02 exited0. Full SDK all-target check04 exited0 after actual API/test
compilation repairs; warnings remain. Hyper's client-only missing classifier
is replaced by bounded plaintext head observation, preserving LimitExceeded;
public HTTP cap final diagnostic passed1 case after recorded failures. See
docs/rust-sdk/unified-validation.md and artifacts/unified/stage116-tooling.
These are partial dirty-tree diagnostics, not whole-candidate acceptance. Whole
Rust behavior, binding/Python, platforms/features/packages/docs/stress/performance
remain. Commit this recovery then continue; do not mark the goal complete.

## Unified validation stage117: full behavior failures and documentation

Full SDK test on7ab55ba ended101 with14 failed targets. All30 full doctests and
all7 serde contracts passed, but overall correctness did not. The lifecycle
test process was stopped only after recording its exact binary/elapsed time;
source diagnosis is an unconsumed lazy spawn iterator before a barrier. Fix
the fixture then verify actual concurrent close. Windows accepted nonblocking
sockets, reset-vs-EOF, invalid expected sum and retained operation context are
also identified; catalog counts, price units, provenance and transport/pin
failures still need evidence. Preserve every failure under stage117-correctness.
Docs attempt01 exposed a stale mirror and links outside MkDocs; actual repairs
gave metadata/MkDocs strict/pages checks exit0 in stage117-docs/attempt02.json.
No task/goal acceptance claimed; continue corrections and full verification.

## Unified validation stage118: partial repairs and proven production failures

Base59888d8; raw per-attempt logs, exact dirty diff hashes and source snapshots
in artifacts/unified/stage118-repairs. F10 four targets67 pass; EX15/boards16/
catalog32/loopback14 pass; lifecycle3 pass, latest lib232 pass1 ignored and HQ
helpers52 pass. Repaired Windows blocking and reset/EOF fixtures, goods cap100
expectations, fixture price double scaling and compensating HQ inputs, readable
row date diagnostic and source URI expectation. Superseded mistaken historical
F37 sum with independent2240001 evidence, algorithms/golden unchanged.
Still actual failures: MAC partial EOF stops failover; worker Failed after
normal cancellation has existing_retirement=true and duplicate begin_retry
fails Internal; pinned download sends fallback1 request after primary2. These
need deterministic red regressions, production fixes and full regression,
not relaxed assertions. No whole/Python/platform/package/stress acceptance.

## Unified validation stage119: production lifecycle and fixed-source repairs

Base74d14b0. Runtime late Failed after cancellation now preserves an existing
exact-wire retirement; deterministic red/green includes rejection of foreign
generation and preservation of original cancelled terminal. Rust pinned child
has one wire attempt on the same authorization/deadline, PythonLegacy unchanged.
Lost-pin public test now passes primary2/fallback0; fixture deliberate shutdown
no longer writes a reply. Partial extension EOF is Connection/UnexpectedEof,
complete malformed replies remain Protocol; five affected targets107 pass.
Combined full attempt02 SDK regular targets/30 doctests, protocol98/runtime129
and deadline5 pass, but overall101 due to pin_handoff fixture missing handshake.
Handshake added and cached25ms broker assertions replaced with real-time pin
identity, queuing and release; pin case attempt03 passes1. Re-run full combined
on the saved checkpoint for whole-command exit0. Original compile/type/IO/move
failures retained in stage119-runtime. WSL Ubuntu22.04 task-specific Rust1.89
install03 passed, no Linux behavior result yet. Full feature/MSRV/compatibility/
consumer/package/docs/stress/performance/production evidence still required.

## Unified validation stage120: Windows combined pass and binding compilation

Clean1d616698971172621eca9f64bb41adf4ebb1f6c8 Windows1.89/full combined SDK,
runtime and protocol test command exited0. SDK lib232 pass1 ignored,30 docs,
7 serde; protocol98/runtime129/deadline5/pin1 and all regular targets passed.
Ignored stress and remaining matrices are not accepted. Python binding check01
failed52 compile errors; explicit numeric/closure types, PyO3 interface and
error-layer repairs reduced check02 to3, check03 exited0 on recorded dirty diff.
KeyboardInterrupt cleanup now distinguishes confirmed result, SDK unconfirmed
retirement and channel failure; actual Python behavior still requires wheel.
Linux01 failed crates.io config connection timeouts;02 uses task-specific cache
copied from existing Windows registry and locked/offline full tests, running
at this checkpoint. Logs stage120-matrix. Next wheel build/isolated consumer,
full Python inventory/contracts/loopback, remaining feature/Loom/platform/package/
stress/performance gates. Goal active unlimited; no release or agents.


## Unified validation stage121: actual Python and cross-platform results

2026年11月17日stage121统一Python与跨平台验收：源码083383113bf297511ed870c508d17f9b4ecfcc4b。Linux Rust1.89/full合并SDK/runtime/protocol在补齐现有crate缓存后locked/offline实际exit0；Windows32feature子集检查exit0。release abi3 wheel构建exit0，wheel SHA256 f5aa24115cc98846eb15b4446741d62425dfccb47dafb9fe771bf07a38e08581。隔离venv安装/pipcheck成功，但公开corporate查询因legacy unpriced admission拒绝而失败，不能算consumer通过。补齐61个未分组文件后122个Python测试文件均归属明确，inventory3通过；golden103通过；完整loopback235通过/41失败，完整local2971通过/17失败。local首轮坏fake目录永不返回空导致无界分页，精确进程记录后停止，修复fake再完整运行；保留失败证据。目录默认1600、普通/退市bar fixture周期与019001旧native解析差异更正后四文件113通过。生产缺口为legacy capital/finance/limits admission与snapshot旧parser、bulk datetime DTO比较和动态transport override；另有测试静态假设/patch作用域/模块导入及全局旧metadata差异待修。原始记录artifacts/unified/stage121-python，各次失败未覆写。30/30仍仅源码范围，整体未验收；目标active无限额，继续修复兼容再跑统一矩阵。


## Stage122 compatibility checkpoint

2026年11月17日stage122兼容修复：基础a512f4a。legacy corporate/finance/limits增加仅Python profile的unpriced准入与旧parser；strict Rust分支不放宽。快照错误原先经RuntimeError转换后失去ProtocolError来源，修复为保留原始typed source并维持旧分类/context，使旧分裂恢复与单条缺失重新生效。ApiBase每次调用核对内置transport原execute身份，类/实例构造前后替换和恢复都受支持，显式native custom adapter仍保留。确定性red后相关208通过；更新native的四集成文件79通过。批量逐笔原测试混比custom datetime blocks与冻结native lazy tuple blocks，现使用原native per-page bulk路径作参考并追加两者全部公开列相等，不改DTO/dataclass或提前物化。修正pytest模块self函数可收集、ST名称normal=N、MAC hook作用域、offline同名function/module导入及三新增ABI tag/共享buffer静态假设。64模块AST增量逐项审计，旧冻结baseline未改：91新增import绑定、38 sdk方法、2私有DTO helper、1私有__all__及三个可选native_services构造参数，反例防旧契约漂移；13项合同通过。候选wheel attempt02成功并核对native hash41d4ef1f977c4f5ec1dfe0e54314bfbe7870adb3c2ca9d410e7f8b192404e054；任务venv安装3.3.5成功，解决系统旧metadata3.3.2，不改生产版本逻辑。实际新完整Python集合/干净wheel消费、受影响Rust重跑及剩余交付矩阵仍待。证据artifacts/unified/stage122-compat，失败轮次保留。goal active无限额。


stage122完整集合复验补充：任务专用venv装入本轮3.3.5 wheel后，完整round4-local实际3046通过、round5-loopback276通过、round3-golden103通过，均exit0；fixture/静态修复没有改旧golden、模型或版本算法。新AST增量清单当时为untracked，runner diff未含该新文件，故单独保存其SHA256至stage122-compat/ast-additions-identity.json并随本检查点提交。受影响pure Rust/新干净wheel消费及其余矩阵仍待，不据此整体验收。


## Stage123 candidate evidence and cancellation race

2026年11月17日stage123干净候选验收：来源92f033b77adb1d041fc819fbeacc3af60d06c9d2，dirty=false。Windows及WSL Ubuntu22.04/Rust1.89的full合并SDK/runtime/protocol命令均exit0；Windows Loom也exit0（runtime131含两模型、deadline5、pin1）。release wheel重新构建成功，独立venv/no-index/pipcheck与非空HQ/F10/EX/MAC/MAC-EX公开消费exit0；隔离仍仅process-path-only，不是物理无编译器镜像。Windows平台矩阵protocol/runtime、minimal、default完成exit0，但full/all-targets在operation::tests::runtime_two_page_comparison_rust_times_out_both_python_profiles_keep_request_windows偶发失败：业务Timeout之后close得到started cancellation was stale。需确定性回归并修生产退役仲裁，不能把先前full通过视为消除竞态。独立严格Clippy失败于protocol四项warning，fmt --check也exit1；保留原始日志。证据artifacts/unified/stage123-candidate，各attempt不覆盖。忽略压力尚未运行，feature/stable/外部Rust包/平台/压力性能与真实服务器证据仍待。目标active无额度限制，无代理或发布。


## Stage124 late cancellation arbitration

2026年11月17日stage124退役仲裁修复：基础b4ffd32。确定性late cancellation回归在生产修复前实际失败started cancellation was stale（red-attempt02；attempt01因遗漏完整模块名而0项，不能算red）。Supervisor只允许精确wire转terminal意图；Runtime取消加入已有清理，先发生的失败终态保留原原因，待retry/endpoint清理则转取消并禁止新attempt，确认仍等待真正ReconnectAck，owner lifecycle清理保留原归属。新用例覆盖普通/pinned、terminal/retry/endpoint六种顺序及外来generation拒绝，green通过。带最终测试增量的Windows Rust1.89合并all-targets实际exit0：SDK lib232/1 ignored、protocol98、runtime130、deadline5、pin1及全部普通integration/examples成功；不包含doctest/ignored压力。证据artifacts/unified/stage124-retirement。Clippy/fmt、Linux/绑定/Python重新构建、完整交付及压力性能仍待。目标active无限额。
