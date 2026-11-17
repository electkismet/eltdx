# eltdx-protocol

TDX wire request/response codecs used by the transport and Rust SDK. This crate
retains the repository's 3.3.5 version line and requires Rust 1.89.0. Its
non-commercial learning and data research [LICENSE](LICENSE) is included in
the package; no permissive license replacement is implied.

Applications wanting business services should use `eltdx`. Protocol types are
lower-level source-native values; wire support alone does not establish a
stable public SDK capability, production server compatibility or units.

This implementation candidate remains uncompiled and untested. Registry name,
version identity, archive contents and downstream consumer acceptance must be
verified before any publication. Tests and large repository fixtures remain in
the source checkout and are not runtime package dependencies.
