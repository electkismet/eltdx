# Rust SDK dependency decisions

## ZIP and CRC — T17, provisional source selection

The default HQ SDK uses one private decoder, `archive::decode_selected`, for
statistics and financial archives. Cargo source declares exact workspace
`zip = 0.6.6` with default features disabled and only `deflate`, and
`crc32fast = 1.5.0`. Both are unconditional SDK dependencies: HQ resource support
does not require the optional offline or F10 feature. The existing flate2 Rust
backend is reused by the selected ZIP dependency; no independent inflater or
filesystem extraction was added.

This is a **provisional, unverified dependency choice**. The local sparse record
read during Stage A declares zip MSRV 1.59.0 (workspace 1.89), an unyanked
0.6.6 entry, and checksum
`760394e246e4c28189f19d488c058bf16f564016aefac5d32bb1f3b51d5e9261`.
Its dependency ranges admit existing flate2 1.1.2 and crc32fast 1.5.0. Expected
license identifiers are MIT for zip and MIT OR Apache-2.0 for crc32fast; exact
license files, advisory state, transitives, target MSRV and APIs have not yet
been verified. No registry resolution, downloads, lockfile regeneration,
compilation or runtime checks have run. Cargo.lock must be resolved and all
requirements verified at the final T01–T30 validation gate before release
readiness can be claimed. A rejected dependency will be replaced at that gate
without weakening the archive limits or introducing a second decoder.

The structural preflight precedes library directory allocation. It admits a
bounded classic Store/Deflate subset, validates central/local declarations and
descriptors, and rejects duplicate raw names, encryption, ZIP64, spanning,
overlapping records and unlisted local data. Streaming checks actual expanded
counts and independently accumulates CRC for every member, including members
not selected for delivery. An operation-supplied boundary is checked between
bounded work units. Library deflate EOF/trailing-byte behavior, interoperability
with independent archives, CRC, malformed streams and limits all await final
validation; authored source cases are not evidence of passing behavior.

| Consumer | Compressed | Entries | Expanded per member | Expanded total |
| --- | ---: | ---: | ---: | ---: |
| HQ statistics | 32 MiB | 128 | 32 MiB | 64 MiB |
| Rust financial archive | 64 MiB | 64 | 256 MiB | 256 MiB |

The financial total is a new Rust protection approved in the implementation
decision register; it must not become a new cap on the legacy Python selected
DAT path. Stats requires both exact unique `tdxstat.cfg` and `tdxstat2.cfg`.
Downloads use a compressed-limit-plus-one allowance, reject oversize before
archive construction, and require transport EOF evidence. This protection is
independent of limits enforced while expanding the archive.

Resource downloads retaining per-response provenance additionally admit at most
65,536 response pages and 128 MiB of retained payload/frame buffers. This bounds metadata/diagnostic overhead for an unusually
small StatsQuery chunk size; reaching it returns LimitExceeded and retires the
pin without claiming EOF. It does not change Python legacy request counts or
limits. Large ignored protocol tails count toward the retained buffer bound as
well as file data; this prevents small content blocks from hiding large raw
allocations. Ordinary byte-only download_file does not retain page metadata. Raw
frames/payloads remain controlled by RawPolicy, and response frames retain the
existing runtime limits. All output reservations are fallible and byte bounded.

## GBK — shared source table

HQ statistics, company information and offline readers share the original
Python 3.12.14 strict GBK scalar table and private scanner. The table was moved,
not regenerated, in Stage A2. Consumers explicitly select replacement or ignore
for invalid source bytes. Company filenames use a bounded reverse lookup of
the same mapping, reject NUL/unrepresentable scalars and enforce encoded byte
limits. The legacy protocol text constructor retains its original encoding
policy; the strict SDK passes encoded bytes through a separate checked
constructor. No second mapping or public caller-injected codec was added.
