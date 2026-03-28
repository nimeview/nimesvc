#!/usr/bin/env bash
set -euo pipefail

echo "nimesvc release build starting..."

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"
if [[ ! -f "$root_dir/Cargo.toml" ]]; then
  echo "Cargo.toml not found at $root_dir"
  exit 1
fi

version="${1:-}"
if [[ -z "$version" ]]; then
  if command -v python3 >/dev/null 2>&1; then
    version="$(python3 -c "import tomllib; f=open('Cargo.toml','rb'); data=tomllib.load(f); print(data.get('package',{}).get('version',''))" || true)"
  fi
  if [[ -z "$version" ]]; then
    version="$(grep -m 1 -E '^version\\s*=\\s*\"' Cargo.toml | sed -E 's/.*\"([^\"]+)\".*/\\1/' || true)"
  fi
fi
if [[ -z "$version" ]]; then
  echo "Unable to detect version. Pass it explicitly: scripts/build-release.sh 0.1.0"
  exit 1
fi
echo "Version: $version"

if ! command -v zig >/dev/null 2>&1; then
  echo "zig not found. Install zig (brew install zig) and re-run."
  exit 1
fi
if ! command -v cargo-zigbuild >/dev/null 2>&1; then
  echo "cargo-zigbuild not found. Install with: cargo install cargo-zigbuild"
  exit 1
fi

ensure_rust_target() {
  local target="$1"
  if rustup target list --installed | grep -qx "$target"; then
    return 0
  fi
  echo "Rust target '$target' is not installed." >&2
  echo "Install it with: rustup target add $target" >&2
  exit 1
}

ensure_rust_target "aarch64-apple-darwin"
ensure_rust_target "x86_64-apple-darwin"
ensure_rust_target "aarch64-unknown-linux-gnu"
ensure_rust_target "x86_64-unknown-linux-gnu"
ensure_rust_target "x86_64-pc-windows-gnu"

out_dir="$root_dir/release"
rm -rf "$out_dir"
mkdir -p "$out_dir"
echo "Output: $out_dir"

build_target() {
  local target="$1"
  local out_name="$2"
  echo "Building $target -> $out_name"
  cargo zigbuild --release --target "$target"
  local bin_path="$root_dir/target/$target/release/nimesvc"
  if [[ "$target" == *"windows"* ]]; then
    bin_path="$root_dir/target/$target/release/nimesvc.exe"
  fi
  if [[ ! -f "$bin_path" ]]; then
    echo "Binary not found: $bin_path"
    exit 1
  fi
  cp "$bin_path" "$out_dir/$out_name"
}

build_target "aarch64-apple-darwin" "nimesvc-macos-arm64"
build_target "x86_64-apple-darwin" "nimesvc-macos-x64"
build_target "aarch64-unknown-linux-gnu" "nimesvc-linux-arm64"
build_target "x86_64-unknown-linux-gnu" "nimesvc-linux-x64"
build_target "x86_64-pc-windows-gnu" "nimesvc-windows-x64.exe"

echo "Building source archives"
if command -v git >/dev/null 2>&1; then
  git archive --format=tar HEAD | gzip -9 > "$out_dir/source-code.tar.gz"
  if command -v zip >/dev/null 2>&1; then
    git archive --format=zip HEAD > "$out_dir/source-code.zip"
  else
    (mkdir -p "$out_dir/source-code" && git archive --format=tar HEAD | tar -x -C "$out_dir/source-code")
    (cd "$out_dir" && zip -qr "source-code.zip" source-code)
    rm -rf "$out_dir/source-code"
  fi
else
  tar \
    --exclude="./target" \
    --exclude="./.git" \
    --exclude="./.nimesvc" \
    --exclude="./release" \
    --exclude="./**/*.log" \
    -czf "$out_dir/source-code.tar.gz" .
  if command -v zip >/dev/null 2>&1; then
    zip -qr "$out_dir/source-code.zip" . -x "target/*" ".git/*" ".nimesvc/*" "release/*" "*.log"
  fi
fi

echo "Release artifacts ready in: $out_dir"
