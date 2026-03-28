Param(
  [string]$Repo = $env:NIMESVC_REPO,
  [string]$InstallDir = $env:NIMESVC_INSTALL_DIR
)

if (-not $Repo) { $Repo = "nimeview/nimesvc" }
if (-not $InstallDir) { $InstallDir = "$env:USERPROFILE\.nimesvc\bin" }

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "AMD64") { $arch = "x64" }
elseif ($arch -eq "ARM64") { $arch = "arm64" }
else { throw "Unsupported arch: $arch" }

$asset = "nimesvc-windows-$arch.exe"
$latest = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$tag = $latest.tag_name
if (-not $tag) { throw "Failed to determine latest release" }

$url = "https://github.com/$Repo/releases/download/$tag/$asset"
$tmp = Join-Path $env:TEMP "nimesvc_update"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$exePath = Join-Path $tmp $asset
Invoke-WebRequest $url -OutFile $exePath
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force $exePath (Join-Path $InstallDir "nimesvc.exe")

$path = [Environment]::GetEnvironmentVariable("Path", "User")
if ($path -notlike "*$InstallDir*") {
  [Environment]::SetEnvironmentVariable("Path", "$path;$InstallDir", "User")
  Write-Host "Added to PATH. You may need to restart your terminal."
}

Write-Host "Installed/updated nimesvc ($tag)"
