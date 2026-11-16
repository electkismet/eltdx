# T10 — 元数据加载、单位投影和时钟

Authority: `docs/rust-sdk/development-plan.md` T10; `architecture.md` §7; `public-api.md` §§7–8; `execution-and-compatibility.md` §4; `hq-api-spec.md` shared type/unit/source contracts; `tests/fixtures/sdk/policy/{normalization,units,dates}.json`. Read authoritative sections and current T08/T09 interfaces before implementation.

## Exact plan task

**文件：** 新增 `src/metadata/{mod,store,snapshot,precision}.rs`，`src/types`；新增 `tests/metadata.rs`、`tests/units.rs`。

**接口：** 私有 market snapshot store，public typed value 投影及来源信息；同 key single-flight，独立等待者取消。

- 市场完整分页后原子发布；中途失败不标记已完成，不清除旧好版本。
- 刷新期间已开始请求仍用旧snapshot；两个owner互不污染。
- 参数化股/手、万元/元、百分比/比值、f32位模式、债券精度、跨日时间；未知单位不猜。
- 注入时钟验证上海日期边界与 metadata 过期，不改系统时钟。
- 最终统一验证：`cargo test -p eltdx --test metadata --test units --locked`。

## Binding integration

- One metadata store owned by `SessionOwner`; same owner's clones/services/pools share it, different owners have no shared mutable table. Key has protocol family, source/catalog namespace, full instrument identity/market. Snapshots are immutable Arc values with checked monotonic version, provenance and fetched/expiry diagnostics. Do not put a network session inside cache or call transport/resolver under its lock.
- Build a candidate from complete market pagination (only actual terminal evidence completes it), validate identities/duplicates/conflicting precision/unit information first, then atomically publish. Incomplete, malformed, timed out or cancelled loading never replaces the last known good snapshot or marks a market complete. Short pages do not imply end. Keep old snapshots alive for running operations during refresh.
- Same-key concurrent loads share one flight/result, yet each waiter observes its own cancellation/owner close/deadline. An early cancelled/timed-out waiter must not abort the load needed by another. Shared work has a bounded independent cleanup/stop responsibility and no worker strong-reference cycle through owner. Source tests must include cancelled original/first waiter with a surviving second waiter, not only cancelled follower. Avoid a hidden global executor.
- T08 operation context captures one snapshot before first decode; `install_metadata` refuses replacement after parsing begins. T09 protocol `PricePrecisionSource` implemented by immutable snapshot. All pages/chunks/fallback decode use this source. Context-aware list decoding must not mutate global legacy table.
- Resolve metadata for strict normalized `SecurityId` in the owner namespace. Python's broader `_ensure_decimals` request selection is a separate compatibility policy, not normal Rust inference. Unit/precision fallback sources distinguish SecurityTable/WireRule/PrefixFallback/UserOverride; classification by prefix is not evidence of existence/listing or exchange rule. Unknowns produce MetadataUnavailable or explicit Unknown units, never guessed shares/yuan/100 divisor. Quote decimal metadata must not alter trades' milli-yuan rule.
- Keep integer original values and scales, original f32/f64 wire source (including bit patterns), and unit provenance. Extend existing Price/Quantity/Amount/Ratio types rather than adding parallel names. Implement checked documented unit conversions only; no converting source to f64 then reconstructing an alleged exact integer. Unknown unit conversion should fail explicitly. Nonfinite business-required fields fail; retained diagnostic f32 raw bits may still be preserved.
- Add stable shared `SourceStamp`, `ResultMeta`, raw page/range helpers only as required by this task and upcoming HQ projection; follow `hq-api-spec.md` §3 rather than exposing wire structs wholesale. If future-domain DTOs remain T11+, don't claim their integration complete here.
- Clock is explicit/injectable for high-level current Shanghai trading-date choice and metadata freshness. System implementation uses UTC epoch plus known +08 offset, never host local timezone. Source-local/naive timestamps remain local with unknown offset; no default UTC. Real Gregorian rollover across month/year/leap dates. Deterministic test clock does not modify process/system time; no test sleeping to imitate date rollover.
- Preserve legacy protocol/Python behavior; new SDK path must not read/write `unit::SECURITY_DECIMALS`. Add source cases for half-load failure, concurrent owners conflicting precision, refresh snapshot freeze, no load on irrelevant commands, audited fallback provenance, raw f32/large exact scaled values, i64::MIN boundary carried from T04, unknown units, Shanghai midnight and metadata expiry.

## Workflow

Worktree `C:/Users/ax/.codex/worktrees/rust-sdk-implementation/tdx抓包`, branch `codex/rust-sdk-implementation`. Controller provides GO after dependency review; do not run tests/collection/build/check/clippy/doctest/network/compile/AST/format or diff checks/equivalent verification until ALL T01–T30 implementation code complete. Read and author source only, ordinary review diff generation and git status/log/add/commit allowed. No subagents, push/tag/publish/merge. No controller progress/status edits. Coordinate shared session/operation files before edits.

Report `.superpowers/sdd/development-plan/task-10-report.md`: full implementation, sources authored, no-execution statement, files, self-review/concerns. Commit exact owned files/report; return compact status/SHAs/no-tests/concerns/path. Do not claim compilation/runtime success.
