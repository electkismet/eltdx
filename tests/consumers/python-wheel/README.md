# Installed Python wheel consumer

This is an **unexecuted source fixture** for T18/T23/T28/T29. Run it only after all
T01–T30 implementation sources are complete. It uses Python's standard library
and public installed `eltdx` imports; it has no repository/test helper imports and
requires neither pytest nor a Rust compiler.

Copy `consumer.py` to an external directory. In a clean virtual environment
with no `rustc`, `cargo` or `maturin` on PATH, install the candidate wheels only:

```sh
python -m pip install --only-binary=:all: --no-index --find-links /absolute/wheelhouse eltdx
python -I /absolute/external/consumer.py --forbidden-root /absolute/source-checkout --require-no-toolchain --evidence /absolute/evidence/python-wheel.json
```

Use matching absolute Windows paths on Windows. Do not add the source checkout to
PYTHONPATH, use editable installation, permit source distribution fallback, or
claim that merely hiding PATH entries proves a compiler is absent from the
machine. The final isolated job/image configuration provides that evidence.
The consumer's `--require-no-toolchain` is an additional runtime assertion.

The fixture listens only on `127.0.0.1`. It checks a nonempty public code-table
collection including its empty terminal page, snapshots with repeated input and
known prices, capital-change chunks with raw bytes and original Python DTOs,
local ping, and close releasing the TCP connection. Its wire data is synthetic
and does not establish any online exchange behavior. Output records installed
version/path/native hashes, platform, Python, observed commands and connections.
The same installed-wheel process now also exercises six actual F10 HTTP POSTs
using only a standard-library local server. It checks generic business errors,
duplicate column names and raw cache metadata, a named stock endpoint, cached
index members, topic merging and explicit topic comparison. The F10 helpers
must leave a separate HQ listener untouched. Python retains its original HTTP
lifecycle; Rust's shared-owner close contract is tested by its own consumer.

Professional financial list and ZIP download use the HQ `0x06B9` resource
fixture. The standard-library ZIP contains an independently authored DAT with
negative zero and a header date distinct from its archive filename. Public
financial models and download filename date precedence must survive. Local ZIP
reading retains the header date, matching the existing local-file contract.
Local-file cases
also cover daily read/encode/append, duplicates, incomplete-tail refusal without
mutation, industry overwrite/order and direct DAT/local ZIP reading. Temporary
files are created only when this consumer is eventually executed.

No success artifact is checked into the source tree: output is written only
after these actual assertions pass in the final validation run.

The separate Rust consumer in `../rust-sdk/` establishes the other installation
direction. These fixtures do not require users to install both language products.
