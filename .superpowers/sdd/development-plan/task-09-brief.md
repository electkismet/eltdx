# T09 — 无全局状态的解析上下文

Authority: `docs/rust-sdk/development-plan.md` T09, `docs/rust-sdk/public-api.md` §§7–8.3, `docs/rust-sdk/execution-and-compatibility.md` §4. Read those sections, not only this brief.

## Exact task

**文件：** 修改 `crates/eltdx-protocol/src/{unit,response}.rs`、相关 `commands`；新增 `context.rs`；新增 `crates/eltdx/src/hq/decode.rs`、`tests/precision_isolation.rs`。

**接口：** protocol 定义 `PricePrecisionSource`/`ParseOptions`/`parse_with_context`；legacy `parse` 暂保留；新 SDK 只用显式路径。

- 同一证券在两个 resolver 提供不同精度，交错/并发解析仍分别得到正确结果。
- 新证券列表解析不写全局；旧入口兼容 wrapper 行为有单独测试。
- 精度依赖整次 operation 捕获的单份 snapshot，所有分页/拆批/fallback 使用同一版本，不在每条请求或记录中重新读可变共享表。
- `include_raw=false` 不创建永久 record hex；对保留字段进行语义和位级对照。
- 最终统一验证：`cargo test -p eltdx-protocol context --locked`、`cargo test -p eltdx --test precision_isolation --locked`。

## Binding design and source pointers

`eltdx-protocol` owns `PricePrecisionSource: Send + Sync`, its own `ParseRawPolicy`, and `ParseOptions<'a>` containing `&'a dyn PricePrecisionSource` plus raw policy. It cannot depend on SDK, metadata store, PyO3, or a mutable process/global/thread-local resolver. New aggregate signature is `CommandResponse::parse_with_context(request: CommandRequest, payload: bytes::Bytes, options: &ParseOptions<'_>) -> Result<Self, ProtocolError>`.

At the start of T09 the aggregate parser in `response.rs` calls legacy functions; `unit::security_decimal` uses a global `OnceLock<RwLock<HashMap<String,u8>>>`, and `price_divisor` reads it before known ETF prefix fallback. `commands/security.rs::parse_security_list_payload` registers precision globally. `commands/quotes.rs` has direct global divisor reads in legacy/snapshot/category/refresh branches, and `commands/minutes.rs` in today/history/recent paths. Inspect actual current sources and preserve old output on old wrappers. Trades' milli-yuan rule must remain independent of quote decimal metadata. Context must reach every nested parser branch including refresh payload XOR handling, not just aggregate dispatch.

Implement common parser bodies accepting explicit context/options; legacy entry points wrap the body with the legacy resolver and existing raw-retention behavior. Existing old list parser preserves its legacy registration, but context-aware list decoding never registers. Avoid copying whole parsing algorithms or using temporary global overrides. Every new SDK decoding path must use injected resolver only. A resolver may implement explicit audited prefix fallback; missing/unverified metadata must not silently become claimed verified precision.

Omit means returned payload bytes empty/unretained and record hex not created at all. RetainPayload shares the supplied Bytes at response level, without copying the whole payload per record; record ranges/offsets and on-demand hex are preferred for new SDK views. RetainWire is handled by SDK raw-reply framing, not invented by protocol decoder. Business-required raw integers, f32 bits and unknown fields survive even when raw diagnostics are omitted. Add context variants to commands that currently unconditionally create record_hex/raw_payload, including corporate, limits, bars, delisted, auction, money flow, historical fund flow, session/resources and trades; don't clear hex only after its allocation. Preserve legacy model shapes required by Python binding. Test SOURCE should compare Omit/Retain semantics and exact raw scalar/unknown bits, not just field counts.

`hq/decode.rs` is a narrow adapter from SDK `RawPolicy` and `RawReply` to context-aware protocol decode with caller-provided immutable precision source. Avoid creating another mutable precision store. T10 installs actual `MetadataSnapshot` into `OperationContext`; in T09 the decode function can take `&dyn PricePrecisionSource` or an immutable Arc snapshot handle independent of concrete store. Do not edit operation/executor while another task owns them: coordinate via controller if needed. Register module through `hq/mod.rs`. Metadata publication/clock APIs belong T10.

Meaningful SOURCE cases: two different resolvers for the same security; interleaved and concurrent decoding; context list decode cannot contaminate either source/global; old list wrapper still registers; quote and minute nested branches honor same explicit source; raw omitted before allocation and retained payload uses shared Bytes; scalar values/unknown bits unchanged. Prefer small independent synthetic payloads or existing fixtures, label provenance honestly.

## Constraints / handoff

Worktree `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`; branch `codex/rust-sdk-implementation`. No tests/collection/build/check/clippy/doctest/network probes/AST or compile scripts/format checks/diff checks/equivalent execution until ALL T01–T30 implementation code is complete. Read and write source only; plain review diff generation, git status/log/add/commit allowed. Do not spawn agents. Do not push/tag/publish/merge. Do not edit controller `progress.md`/`IMPLEMENTATION_STATUS.md`.

Report `.superpowers/sdd/development-plan/task-09-report.md`: implementation, files, authored source cases, no execution, self-review concerns. Commit exact owned files/report, no unrelated files. Return short status/commit/no-test statement/concerns/report path. No compilation or runtime success claims.
