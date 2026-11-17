# T29 implementation report

## Stage 1 — versions, package manifests and archive consumer (stage110)

Base1cfa649,2026-11-17. SDK candidate is now explicitly0.1.0; lower libraries
and Python retain3.3.5. Added exact path+version constraints throughout the
internal graph; binding publish=false. Each Rust library has package README,
explicit include inventory and a byte-for-byte copy of the original license.
No license terms were changed. Cargo.lock resolution is intentionally deferred.
Registry names/version availability/ownership have not been queried or claimed.

Authored rust_sdk_packages.py and a three-platform archive-consumer CI source.
It captures clean candidate/toolchain/lock/command logs and hashes; creates a
fresh staging workspace with explicit lower patches, lists/packages all3, notes
--no-verify, inspects/extracts archives and checks normalized dependencies,
required README/LICENSE/src. The independent consumer uses only archive paths
via explicit3-package patches and new lock/target, running minimal/default/full
and all8 modes. This is only an authored local simulation, not registry proof.
If Cargo still cannot resolve unpublished lower candidates, preserve failure
and supply a local registry in final repair; never remove version constraints.

Registry observation sources record actual HTTP/time/raw/hash/version data.
Registry dry-run first downloads each published lower version, verifies registry
checksum, original license and exact candidate src files; source mismatch or
missing/yanked versions fails and requires version selection. Ordinary package
and publish --dry-run then remain nonpublishing checks. Postpublication consumer
with no patches is a separate future acceptance requirement, not claimed here.
Release policy fixes independent versions, Python compatibility, wheel-only
installation versus source build, package contents, license and artifact identity.

All scripts/workflows remain unexecuted. No dependency/network/download, test,
collection/build/check/compiler/AST/format/package/install or equivalent check
ran. Names, API/MSRV/platform/package/consumer validity remain unproven. No
agents, push/tag/publish/merge/controller edits. T29 source scope complete;
28 source-complete/1 partial(T19)/1 unstarted(T30). Next complete stress/performance/
coverage/maintenance sources and T19 source handoff; then unified verification.
Goal active unlimited; production MAC-EX and optional-setup evidence still absent.
