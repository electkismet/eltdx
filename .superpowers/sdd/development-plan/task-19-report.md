# T19 source progress — F10 foundation

## First checkpoint, 2026-11-17

T18 source gate was committed as `515a13c`. T19 is now in progress. All test,
build, check, network, dependency resolution and execution validation remain
forbidden until T01–T30 implementation is complete. No such execution ran.

Authored `crates/eltdx/src/f10/{mod,config,json,request,response}.rs`, feature
registration, optional serde_json/url declarations and `tests/f10_values.rs`.

- Ordered duplicate raw JSON keys, exact numeric lexemes with checked scalar
  access, BOM/UTF-8, compact Unicode encoding, byte/depth/node limits and
  caller-supplied operation boundaries. Strings alone use serde_json's decoder.
- Private config fields and default three-route selection, no ambient proxy,
  validated header/URL/proxy types, TLS/resolver policies and source limits.
- Mutually exclusive Body/Params requests; typed extra keys reject collisions
  before mutation. Params encoding avoids cloning the complete request tree.
- Ordered multi-table/cell response, duplicate column access and Python suffix
  collision projection, absent/empty/null distinctions, business error status.
  Raw Omit does not clone a complete response tree for retention.
- Authored public and private source cases for these contracts, unexecuted.

## Next work — not implemented yet

1. F10ClientBuilder/F10Client with independent lazy SessionOwner and the common
   OperationContext; live configuration and diagnostic snapshots, close/no reopen.
2. Narrow domain-neutral RequestPermit authorization consumption. Start the
   unique reserved physical attempt at connect/send, sharing existing counters.
3. Choose bounded DNS plus production HTTP/TLS transport after source review;
   implementation must provide per-phase cancellation and confirmed cleanup.
   Hyper/rustls local cached sources were read only; no dependency was selected.
4. Implement streaming header/encoded/decoded caps, gzip/chunks, explicit proxy
   CONNECT, root/hostname verification, IPv4 preference/IPv6 fallback and exact
   source retention. No implicit environment proxy/cookie/retry.
5. All required `f10_http.rs` local HTTP/HTTPS/proxy/fault/close fixture source,
   dependency decision and integration handoff to T20/T23. Do not run them yet.

Current totals: **19 source complete, 4 partial (T19/T22/T24/T25), 7 unstarted
(T20/T23/T26–T30)**. Goal active without token cap. No subagents or publication.
Neither F10 nor the whole SDK is claimed usable/compilable/verified.
