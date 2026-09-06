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
    # target/release: for cargo run / dev launches next to the exe.
    Copy-Item $dll "$root\target\release\" -Force
    # apps/desktop: bundled into the installers via bundle.resources.
    Copy-Item $dll "$root\apps\desktop\" -Force
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
# Strip ALL whitespace: the key file wraps base64 across lines, and a .cmd
# wrapper cannot receive multi-line arguments.
$key = ((Get-Content $keyFile -Raw) -replace '\s', '')
foreach ($f in @($nsi, $msi)) {
  # PowerShell cannot hand an empty-string --password through the .cmd
  # wrapper, so this may fail; signing then happens via bash
  # (scripts/upload flow). Keep the build result usable either way.
  & "$root\apps\desktop\ui\node_modules\.bin\tauri.cmd" signer sign --password "" -k $key $f
  if ($LASTEXITCODE -ne 0) {
    Write-Warning "signing failed for $f — sign via bash: tauri signer sign --password `"`" -k (key) file"
  } else {
    Write-Host "Signed $f"
  }
}
Write-Host "Bundle complete: v$ver"
