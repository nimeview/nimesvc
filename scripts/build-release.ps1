$ErrorActionPreference = "Stop"

$rootDir = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $rootDir

$version = $args[0]
if (-not $version) {
  $line = Get-Content -Path (Join-Path $rootDir "Cargo.toml") | Where-Object { $_ -match '^version\s*=\s*"' } | Select-Object -First 1
  if ($line) {
    $version = $line -replace '.*"([^"]+)".*', '$1'
  }
}
if (-not $version) { throw "Unable to detect version. Pass it explicitly: scripts\\build-release.ps1 0.1.0" }

if (-not (Get-Command zig -ErrorAction SilentlyContinue)) {
  throw "zig not found. Install zig and re-run."
}
if (-not (Get-Command cargo-zigbuild -ErrorAction SilentlyContinue)) {
  throw "cargo-zigbuild not found. Install with: cargo install cargo-zigbuild"
}

function Ensure-RustTarget([string]$target) {
  $installed = rustup target list --installed
  if ($installed -contains $target) {
    return
  }
  throw "Rust target '$target' is not installed. Install it with: rustup target add $target"
}

Ensure-RustTarget "aarch64-apple-darwin"
Ensure-RustTarget "x86_64-apple-darwin"
Ensure-RustTarget "aarch64-unknown-linux-gnu"
Ensure-RustTarget "x86_64-unknown-linux-gnu"
Ensure-RustTarget "x86_64-pc-windows-gnu"

$outDir = Join-Path $rootDir "release"
if (Test-Path $outDir) { Remove-Item -Recurse -Force $outDir }
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function Build-Target([string]$target, [string]$outName) {
  Write-Host "Building $target -> $outName"
  cargo zigbuild --release --target $target
  $binPath = Join-Path $rootDir "target/$target/release/nimesvc"
  if ($target -like "*windows*") { $binPath = Join-Path $rootDir "target/$target/release/nimesvc.exe" }
  if (-not (Test-Path $binPath)) { throw "Binary not found: $binPath" }
  Copy-Item -Force $binPath (Join-Path $outDir $outName)
}

Build-Target "aarch64-apple-darwin" "nimesvc-macos-arm64"
Build-Target "x86_64-apple-darwin" "nimesvc-macos-x64"
Build-Target "aarch64-unknown-linux-gnu" "nimesvc-linux-arm64"
Build-Target "x86_64-unknown-linux-gnu" "nimesvc-linux-x64"
Build-Target "x86_64-pc-windows-gnu" "nimesvc-windows-x64.exe"

Write-Host "Building source archives"
if (Get-Command git -ErrorAction SilentlyContinue) {
  git archive --format=tar HEAD | & tar -x -C $outDir 2>$null
  git archive --format=tar HEAD | & tar -czf (Join-Path $outDir "source-code.tar.gz") -C $rootDir .
  git archive --format=zip HEAD > (Join-Path $outDir "source-code.zip")
} else {
  & tar -czf (Join-Path $outDir "source-code.tar.gz") -C $rootDir .
  if (Get-Command Compress-Archive -ErrorAction SilentlyContinue) {
    Compress-Archive -Path (Join-Path $rootDir "*") -DestinationPath (Join-Path $outDir "source-code.zip")
  }
}

Write-Host "Release artifacts ready in: $outDir"
