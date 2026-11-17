#!/usr/bin/env bash
# Intended for a clean Rust-only container, after all implementation sources.
set -euo pipefail
if [[ ${ELTDX_IMPLEMENTATION_COMPLETE:-} != yes ]]; then
  echo 'Finish T01-T30 sources and set ELTDX_IMPLEMENTATION_COMPLETE=yes.' >&2
  exit 2
fi
source_root=${1:?source checkout required}
destination=${2:?new empty external directory required}
for executable in python python3 pip pip3 maturin; do
  if command -v "$executable" >/dev/null 2>&1; then
    echo "Rust-only environment unexpectedly exposes $executable" >&2
    exit 1
  fi
done
if [[ -e "$destination" ]]; then
  echo 'External destination must not exist; evidence is never overwritten.' >&2
  exit 2
fi
mkdir -p "$destination/candidate" "$destination/consumer" "$destination/evidence"
cp -R "$source_root/crates" "$destination/candidate/crates"
cp -R "$source_root/.cargo" "$destination/candidate/.cargo"
for source_file in Cargo.toml Cargo.lock rust-toolchain.toml LICENSE README.md; do
  cp "$source_root/$source_file" "$destination/candidate/$source_file"
done
cp -R "$source_root/tests/consumers/rust-sdk/." "$destination/consumer/"
# Use a new consumer workspace outside the checkout and its own lock/target.
sed 's|path = "../../../crates/eltdx"|path = "../candidate/crates/eltdx"|' \
  "$destination/consumer/Cargo.toml" > "$destination/consumer/Cargo.toml.tmp"
mv "$destination/consumer/Cargo.toml.tmp" "$destination/consumer/Cargo.toml"
cd "$destination/consumer"
export CARGO_TARGET_DIR="$destination/target"
exec > "$destination/evidence/consumer.log" 2>&1
trap 'result=$?; printf "%s\n" "$result" > "$destination/evidence/exit-code.txt"; sha256sum "$destination/evidence/consumer.log" > "$destination/evidence/consumer.log.sha256"' EXIT
rustc -Vv
cargo -Vv
uname -a
sha256sum "$destination/candidate/Cargo.lock"
cargo generate-lockfile
cargo build --locked --no-default-features
cargo run --locked --no-default-features -- lazy
cargo run --locked --no-default-features -- loopback
cargo run --locked -- lazy
cargo run --locked -- loopback
for feature in sdk-serde sdk-offline sdk-f10 sdk-ex sdk-mac sdk-full; do
  cargo build --locked --no-default-features --features "$feature"
done
for mode in lazy loopback offline financial f10 ex mac mac-ex; do
  cargo run --locked --no-default-features --features sdk-full -- "$mode"
done
cargo tree --locked --no-default-features --edges normal,build > "$destination/evidence/minimal-tree.txt"
cargo tree --locked --no-default-features --features sdk-full --edges normal,build > "$destination/evidence/full-tree.txt"
if grep -E '(eltdx-python|pyo3(-ffi|-build-config)?|maturin) v' "$destination/evidence/"*-tree.txt; then
  echo 'Python dependency found in Rust consumer closure.' >&2
  exit 1
fi

# Check the disabled-module contract with rustc diagnostics, after a known-good
# baseline. A dependency/download/manifest failure cannot count as exclusion.
mkdir -p "$destination/negative/src"
cat > "$destination/negative/Cargo.toml" <<'MANIFEST'
[package]
name = "eltdx-disabled-feature-consumer"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
eltdx = { path = "../candidate/crates/eltdx", default-features = false }
MANIFEST
printf '%s\n' 'fn main() { let _ = eltdx::TdxClient::builder(); }' > "$destination/negative/src/main.rs"
cargo generate-lockfile --manifest-path "$destination/negative/Cargo.toml"
cargo check --locked --manifest-path "$destination/negative/Cargo.toml"
for module in offline f10 ex mac; do
  printf 'use eltdx::%s;\nfn main() {}\n' "$module" > "$destination/negative/src/main.rs"
  if cargo check --locked --manifest-path "$destination/negative/Cargo.toml" > "$destination/evidence/disabled-$module.log" 2>&1; then
    echo "Disabled module $module was publicly accessible." >&2
    exit 1
  fi
  grep -F 'error[E0432]' "$destination/evidence/disabled-$module.log"
  grep -F "no \`$module\` in the root" "$destination/evidence/disabled-$module.log"
  cargo check --locked --manifest-path "$destination/negative/Cargo.toml" --features "eltdx/$module"
done
sha256sum Cargo.lock
echo 'Rust-only external consumer and feature exclusion checks passed.'
