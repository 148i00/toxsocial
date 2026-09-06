# Build ToxSocial Windows installer with required runtime DLLs.
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/bundle.ps1
# Make sure your proxy env vars are set if you need to download WiX/NSIS.
#
# NOTE on updater signing: tauri build cannot sign non-interactively on
# Windows — the empty passphrase cannot be expressed as an env var (Windows
# drops empty env vars), so the CLI blocks on a password prompt. We therefore
# build WITHOUT the key env and sign the finished installers explicitly with
# `tauri signer sign --password ""` (the key's passphrase is empty).

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
  # Signing errors are expected here (password prompt); installers may still
  # have been produced. Real failure detection happens below via artifacts.
} finally {
  Pop-Location
}

$ver = (Get-Content "$root\apps\desktop\tauri.conf.json" -Raw | ConvertFrom-Json).version
$nsi = "$root\target\release\bundle\nsis\ToxSocial_${ver}_x64-setup.exe"
$msi = "$root\target\release\bundle\msi\ToxSocial_${ver}_x64_en-US.msi"
if (-not (Test-Path $nsi)) { throw "build failed: missing $nsi" }
if (-not (Test-Path $msi)) { throw "build failed: missing $msi" }

$keyFile = "$env:USERPROFILE\.toxsocial\updater.key"
if (-not (Test-Path $keyFile)) { throw "updater key not found: $keyFile" }
$key = (Get-Content $keyFile -Raw).Trim()
foreach ($f in @($nsi, $msi)) {
  & "$root\apps\desktop\ui\node_modules\.bin\tauri.cmd" signer sign --password "" -k $key $f
  if ($LASTEXITCODE -ne 0) { throw "signing failed: $f" }
  Write-Host "Signed $f"
}
Write-Host "Bundle complete: v$ver"
