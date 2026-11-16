# T25 Stage A — MAC bitmap, command codecs and source-native models

This is an isolated preparatory checkpoint for T25, not the full T25 authorization. The authoritative contract is `task-25-brief.md`, `docs/rust-sdk/extension-api-spec.md` §§1,4,5, and the original Python MAC command/bitmap/model sources. Read all of those relevant parts before editing. T24 Stage A wire codecs are complete and statically approved through `04f613a`; no public EX/MAC session or shared admission hook exists yet.

## Owned Stage A paths only

- New files under `crates/eltdx/src/mac/` for checked MAC-native IDs, bitmap definitions/selection/encoding/decoding, source-value/slot models, and pure MAC/MAC-EX command payload codecs.
- New `crates/eltdx/tests/mac_wire.rs` with literal source cases.
- `.superpowers/sdd/development-plan/task-25-report.md`.

Do not edit root/Cargo/features, `ex/`, protocol/runtime/shared types, owner/operation/session, Python, T10/T11 or other agents' paths. Do not add network client stubs. Any public surface remains unregistered until Stage B GO.

## Required source behavior

- Cover every simple command payload/schema promised in T25, while identifying T26-owned aggregate/composite methods explicitly. Preserve exact request widths/field ordering/defaults and response cardinality, checked counts/ranges and malformed-versus-empty outcomes.
- Implement source-defined MAC 10-byte `<BIBHH>` request envelope and 16-byte response head parsers as pure values, but keep stream sequencing, handshake publication, serialized I/O and session retirement for Stage B. Never decode EX setup/auth by matching port.
- Bitmap field IDs 0..150 and exact 20-byte shape. Every set legal bit consumes four response bytes, including unknown bit IDs. Reserved 151..159 do not consume value slots; preserve observed reserved/control flags. Field selection is a deduplicating bitset; presets/default schemas must match source. 0x122C exclude mask rejects bits 128..150 instead of truncating. Preserve field aliases with contextual semantics, native scalar type and float bits. Retain all required unknown slots under Omit. AH_CODE retains raw u32 and derives padded text only with its explicit market argument.
- MAC-EX default schema is exactly seven fields `PRE_CLOSE, OPEN, HIGH, LOW, CLOSE, VOL, AMOUNT`, distinct from the broader MAC quote default. Use the EX frame/command layer only later when registered; Stage A may encode/decode its own typed payload layouts but may not copy EX model/session/transport code.
- Validate MAC u16 market and MAC code encoding/22-byte boundary independently from HQ/EX identity. Preserve unknown unit/evidence rather than infer A-share conventions. Preserve period13 seconds and time/date provenance. Preserve full wire/native bits for f32 and position-u32 dual interpretation.
- Source has no T24 Stage B profile/session yet. Therefore do not claim associated frame transmission, owner lifecycle, request/reply gate, MAC-EX login-before-ready, host routing, retries/failover, operation budget, public method registration, or T26 composite coverage. State exact seams and defer them.
- Every authored test remains unexecuted. Do not run tests, collection, build/check/clippy/doctest, dependency resolution, network, compiler/AST/format/diff checks or equivalent validation until ALL T01–T30 implementation code is complete.

## Handoff/report

Write the full source contract map, codecs implemented, native schema/bitmap map, evidence gaps, Stage B exact files/seams, T26/T27 handoff, authored cases, self-review and explicit no-execution status to `.superpowers/sdd/development-plan/task-25-report.md`. Commit exact owned paths with hooks disabled. Return compact status, SHA, limitations and report path. No subagents, push/tag/publish/merge, or status/ledger edits.
