# Installed Python wheel consumer

This is an **unexecuted source fixture** for T18/T28/T29. Run it only after all
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
No success artifact is checked into the source tree: output is written only
after these actual assertions pass in the final validation run.

The separate Rust consumer in `../rust-sdk/` establishes the other installation
direction. These fixtures do not require users to install both language products.
