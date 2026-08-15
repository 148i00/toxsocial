# Build ToxSocial Windows installer with required runtime DLLs.
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/bundle.ps1
# Make sure your proxy env vars are set if you need to download WiX/NSIS.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

$dlls = @(
  "$env:USERPROFILE\vcpkg\installed\x64-windows\bin\libsodium.dll",
  "$root\build\c-toxcore\vcpkg_installed\x64-windows\bin\pthreadVC3.dll"
)

New-Item -ItemType Directory -Force -Path "$root\target\release" | Out-Null
foreach ($dll in $dlls) {
  if (Test-Path $dll) {
    Copy-Item $dll "$root\target\release\" -Force
    Write-Host "Copied $dll"
  } else {
    Write-Warning "Missing DLL: $dll"
  }
}

Push-Location "$root\apps\desktop"
try {
  & "$root\apps\desktop\ui\node_modules\.bin\tauri.cmd" build @args
  if ($LASTEXITCODE -ne 0) { throw "tauri build failed with exit code $LASTEXITCODE" }
} finally {
  Pop-Location
}
