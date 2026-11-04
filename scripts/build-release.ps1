<#
.SYNOPSIS
  Builds BoilerSteamXL's Windows release artifacts into dist-release\:
    BoilerSteamXL-Setup-Win64.exe     (NSIS installer)
    BoilerSteamXL-Portable-Win64.zip  (extract and run)
    SHA256SUMS.txt

.DESCRIPTION
  Needs Rust (MSVC or GNU toolchain), Node.js 20.19+/22.12+, and for the GNU toolchain
  MinGW-w64 on PATH. Run from any folder:  powershell -File scripts\build-release.ps1
  The binaries are not code-signed unless you sign them afterwards.
#>
param([switch]$SkipTests, [switch]$SkipBuild)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$app = Join-Path $root "apps\boilr-tauri"
$out = Join-Path $root "dist-release"

if (-not $SkipTests) {
  Push-Location $root
  cargo test -p bsxl-engine
  if ($LASTEXITCODE -ne 0) { throw "Engine tests failed" }
  Pop-Location
  Push-Location $app
  cargo test
  if ($LASTEXITCODE -ne 0) { throw "App tests failed" }
  Pop-Location
}

if (-not $SkipBuild) {
  Push-Location $app
  npm ci
  if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
  npx tauri build
  if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
  Pop-Location
}

$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $app "target" }
$exe = Join-Path $target "release\BoilerSteamXL.exe"
$setup = Get-ChildItem (Join-Path $target "release\bundle\nsis") -Filter "BoilerSteamXL_*_x64-setup.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not (Test-Path $exe)) { throw "Missing $exe" }
if (-not $setup) { throw "No NSIS installer was produced" }

New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $setup.FullName (Join-Path $out "BoilerSteamXL-Setup-Win64.exe") -Force

# Portable edition: the same executable plus a marker that keeps data next to it.
$stage = Join-Path $out "portable"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe $stage
# GNU-toolchain builds load WebView2Loader.dll at runtime (MSVC builds link it statically).
$loader = Join-Path (Split-Path $exe) "WebView2Loader.dll"
if (Test-Path $loader) { Copy-Item $loader $stage }
@"
This file makes BoilerSteamXL portable: settings, the review queue, logs and backups are
kept in the BoilerSteamXL-data folder next to BoilerSteamXL.exe. Delete this file to use
%APPDATA%\BoilerSteamXL instead. If this folder is read-only, BoilerSteamXL falls back to
%LOCALAPPDATA%\BoilerSteamXL-portable and says so in the app.
"@ | Set-Content -Encoding utf8 (Join-Path $stage "BoilerSteamXL.portable")
Copy-Item (Join-Path $root "packaging\README-Portable.txt") $stage
Copy-Item (Join-Path $root "LICENSE-MIT.txt"), (Join-Path $root "LICENSE-APACHE.txt"), (Join-Path $root "NOTICE.md") $stage
$zip = Join-Path $out "BoilerSteamXL-Portable-Win64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Remove-Item -Recurse -Force $stage

Get-ChildItem $out -File | Where-Object { $_.Extension -in '.exe', '.zip' } | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
} | Set-Content -Encoding ascii (Join-Path $out "SHA256SUMS.txt")

Write-Host "Artifacts in $out"
Get-ChildItem $out | Format-Table Name, Length
