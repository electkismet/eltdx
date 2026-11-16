# T23 F10/offline/financial compatibility implementation

## First checkpoint — shared financial list row parser

Previous source checkpoints: T20 `1722815`; T22 `6885e00` and completion
`df29d6b`. Read both implementation indexes and the task brief before recovery.

The native binding now enables the existing SDK `offline` feature. Added a
shared borrowed row iterator to offline/financial.rs, used by both the strict
list parser and Python legacy candidate extraction. New PyO3 financial_file_list
projects rows with the actual Python int callable supplied by the module.
This preserves arbitrary precision, negative sizes, runtime Unicode decimal
digits and the original rule of skipping only ValueError. Other errors escape
unchanged. Standard exact bytes use this entry; custom bytes/decode objects
retain the Python adapter. Empty input still returns before loading native.

Files: crates/eltdx/src/offline/financial.rs;
crates/eltdx-python/Cargo.toml and src/{lib,offline_financial}.rs;
src/eltdx/offline/history_financial.py;
tests/test_native_financial_contract.py (four authored source cases).

Cases cover Unicode/sign/arbitrary-size conversion, invalid integer skipping,
replacement/whitespace, custom decode, overridden int and exception behavior.
None has been collected, compiled or executed. No capability manifest status
is promoted based only on this source work.

Next: DAT shares the existing parser algorithm while preserving Python raw
date objects and exact old error messages; ZIP first-member/entry policy and
all remaining offline codecs, F10 methods, cross-source helpers and manifest
evidence remain required. Full T23 completion is not claimed. Counts21 source
complete/4 partial/5 unstarted; active unlimited goal, no execution checks,
agents or publication.
