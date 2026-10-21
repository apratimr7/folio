# Build folio.exe on Windows and zip it with the README and license.
#
#   ./scripts/build-windows.ps1
#
# Requirements: Visual Studio 2022 (or Build Tools) with the C++ workload, LLVM/clang (for the MuPDF bindings,
# e.g. `winget install LLVM.LLVM`), and rustup. MuPDF is compiled in statically.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$dir = "target/release/folio-$version-windows"
Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item target/release/folio.exe, README.md, LICENSE $dir
$zip = "target/release/folio-$version-windows-x86_64.zip"
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path $dir -DestinationPath $zip
Write-Host ""
Write-Host "built $dir\folio.exe"
Write-Host "      $zip"
