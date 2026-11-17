# T27 implementation report

## Stage 2 — bitmap building and local adjustment (2026-11-17, stage89)

Stage1 committed ebcb278. Counts remain25 source-complete/2 partial/3
unstarted. Added native build_bitmap/build_exclude_flags/_post_ah_code and
all three adjustment helpers. Shared write_bit is used by strict bitmap
encoding and Python's ordinary integer builder. Custom widths/bits/numeric
methods preserve the Python path without a new cap; live normalizer and
bytearray/exception constructor timing remain visible. Strict and legacy
A/H code projections share the response-market width decision.

Strict and Python adjustment use the same denominator/numerator operation
order, OHLC mutation traversal and invalid-price short-circuit primitive.
Strict finite-input/output validation stays strict; Python retains original
NaN policy, stable datetime/event sorting, original date/bisect/compute/
math/range/enumerate/model hooks, per-column attribute collection order,
custom inplace numeric methods, shallow-copy no-event path and newly built
DTOs for selected but all-skipped events. Python sorting key functions are
explicit tiny object-access adapters, retaining lambda identity/type for
custom sorted hooks rather than routing business back into Python.

Added14 public Python contract functions (now30, two parametrized) and one
private Rust source function; expanded the shared bit writer fixture. Cases
cover normal/debug/high-bit/error layouts, dynamic32B/high200 bit, exact
double .value access, custom shift/inplace-or and captured bytearray,
exclude object/size, A/H membership/str, NaN/numeric-factor order, no-event
shallow identity/empty early return, same-date sequential adjusted closes,
all-skipped DTO rebuilding, live reverse range/inplace OHLC/model and eager
OHLC tuple before finite short-circuit. Also fixed source-read-observed
captured int before custom board slicing, and live FieldBit reread after
registry.get; both have authored contract cases.

No tests/collection/build/check/dependencies/network/compiler/AST/format/
diff checks ran. Source-only compatibility remains unverified; PyO3/generic
signatures and actual wheel import must be checked only after all source
scope is complete. Full client/transport/config/generation/command/parser/
catalog/manifest/consumer scope of T27 remains. Pure DTO declarations and
enum registries stay as original public object adapters, with business
methods still to be individually mapped. Goal active unlimited; no agents
or publication. Continue complete T27 and T28-T30, T19 final source handoff,
then unified execution/fixes and production protocol evidence.

## Stage 1 — board/selection/bitmap compatibility (2026-11-17, stage88)

Base5593729. T27 is started, not source-complete. Totals are25
source-complete/2 partial(T19/T27)/3 unstarted(T28-T30). Goal active with
no quota. Full implementation must continue before any execution check.

The binding SDK dependency now enables mac (implies ex). A new
extension_bitmap.rs registers seven functions and a private iterator.
Python convert_board_code, FieldSelection.__init__, normalize_fields,
_field_bits, count_set_bits, iter_set_bits and get_active_fields delegate
their business flow into native Rust. Python public DTO/enum/preset objects
remain the existing objects. Strict Rust BoardSymbol and Python conversion
share board_code_plan; strict decode_bitmap and Python ordinary bit
iteration share next_set_bit. Strict input/size/overflow limits do not leak
into Python's arbitrary integer and dynamic FIELD_BIT_LIMIT policies.

Preserved source decisions: strip then US/HK then len6 and88/399/899/000;
actual slice(2,None,None), live int/len and left/right arithmetic order;
FieldSelection live set/tuple/isinstance, ordered dedup/original references;
normalize identity and real tuple star expansion/length hints without adding
a module tuple lookup; mask before live int.from_bytes; iteration defers
conversion until first next, resolves live registry/fmt between yields,
counts unknown legal slots, ignores masked reserved bits, and retires after
errors. Standard exact nonnegative integers use the shared Rust byte scan;
custom/negative _field_bits results preserve lowbit Python numeric methods,
including __ixor__, without an invented finite iteration cap. Outer Python
iter_set_bits remains a generator using yield from.

Authored tests/test_native_extension_contract.py with16 public test
functions (first parametrized): Unicode/signed/huge board integers, strip
slice and short-circuit order, live len/arithmetic/error identity, selection
ordering/aliases/live set/tuple, normalize identity/star/iteration errors,
unknown/reserved/arbitrary bitmap width, deferred evaluation/live registry,
dynamic limit/mask conversion order, custom inplace xor, negative lazy
results, generator exception closure and live count/active helpers. Added
two public Rust mac_routing functions for shared scanner and plan source
contracts. None have been collected, compiled or executed.

Not migrated yet: build_bitmap/exclude_flags; adjustment/helper/model
business; all EX/MAC/MAC-EX ordinary business orchestration, management,
configuration, connection generation, raw/exception/custom transport
integration; full manifest and installed-wheel consumers. Existing protocol
bridge alone is not sufficient proof of those migrations. Continue the full
task-27-brief.md scope; do not replace it with this helper substage.

Source-reviewed risks: PyO3 method/lifetime signatures remain uncompiled;
Python star expansion edge errors must be assessed at final execution;
production MAC optional-frame boundaries/MAC-EX login proof still absent.
No production ACK was invented. Runtime/native ABI issues remain deferred.

Files: crates/eltdx-python/{Cargo.toml,src/lib.rs,src/extension_bitmap.rs},
crates/eltdx/src/mac/{bitmap,enums}.rs, src/eltdx/mac/{bitmap,client}.py,
tests/test_native_extension_contract.py, crates/eltdx/tests/mac_routing.rs,
canonical progress/status/plan and their package mirrors. Recovery commit
contains exact file changes; its SHA is recorded by the following stage.

No tests/build/check/collection/clippy/doctests/dependency resolution or
download/network/compiler/AST/format/diff checks. Ordinary source reads,
edits and Git review only; checkpoint disables hooks to preserve progress.
No agents, controller edits, push, tags, publication or merge.
