# Rust SDK Fixtures

`inventory.json` records the source and evidence for every legacy 7709 fixture
and every supplemental SDK case. The inventory generator reads the committed
inputs and expected files; it never imports the Python parser or regenerates an
expected result.

The 37 legacy fixtures were materialized from synthetic inputs at
`401dda95dd337ddf1440c18629be37d39d134d2c`. Their `expected.json` files are
Python regression baselines generated at
`0fb9d00ebbb2dabc1f40f511919a192003896e36`, not independent protocol oracles.
`delisted_bars/normal` is a special case: its record bytes reference
`052b-ui-20261005/live-query-example.json`, which is not present in this
repository; only the synthesized request/response wrapper and parsed baseline
are stored here.

Files under `cases/` are language-neutral contracts. `request` and `response`
describe the inputs without calling a Rust or Python SDK implementation.
`expected.python_compat` freezes the old public behavior; `expected.rust`
states the planned native contract. Evidence levels distinguish hand-derived
wire values from compatibility expectations. Structured ZIP members are
archive-construction inputs, not claimed captured ZIP bytes.

Known real-input gaps are listed in `inventory.json`. In particular, no
non-empty `0x052d` category-22 server response is fabricated, and that
experimental path remains separate from stable `0x0ffc` daily flow and the
7615 F10 history endpoint.

Files under `policy/` freeze date, unit, code-identity, price-limit, and legacy
compatibility decisions before the Rust SDK exists. Their expected values are
source-reviewed contracts, arithmetic examples, or explicit evidence gaps;
they are not live captures or authoritative exchange rules. Price-limit
profiles remain unapproved until dated regulatory/exchange sources and boundary
samples are recorded. `tests/contracts/test_sdk_policy_fixtures.py` describes
the contract checks; execution is deferred until all T01-T30 implementation
code is complete.
