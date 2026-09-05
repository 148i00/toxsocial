# Generate the Tauri updater manifest (latest.json) for a release and upload
# it next to the installers. Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/make-latest-json.ps1 -Version 0.2.31
# Requires: the release with the installers + .sig files already uploaded.

param(
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$Repo = "148i00/toxsocial"
)

$ErrorActionPreference = "Stop"
$base = "https://github.com/$Repo/releases/download/v$Version"
$nsi = "ToxSocial_${Version}_x64-setup.exe"
$nsiSig = Get-Content "target\release\bundle\nsis\$nsiSigPath" -ErrorAction SilentlyContinue

$nsiSigFile = "target\release\bundle\nsis\$nsi.sig"
$msiSigFile = "target\release\bundle\msi\ToxSocial_${Version}_x64_en-US.msi.sig"
if (-not (Test-Path $nsiSigFile)) { throw "missing signature: $nsiSigFile" }
if (-not (Test-Path $msiSigFile)) { throw "missing signature: $msiSigFile" }

$nsiSignature = (Get-Content $nsiSigFile -Raw).Trim()
$msiSignature = (Get-Content $msiSigFile -Raw).Trim()
$pubDate = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")

$manifest = [ordered]@{
  version   = $Version
  notes     = "ToxSocial v$Version"
  pub_date  = $pubDate
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = $nsiSignature
      url       = "$base/$nsi"
    }
  }
}
# The MSI is also signed; Tauri v2 updater on Windows consumes the NSIS
# artifact, so the MSI signature is uploaded alongside for manual installs.
$json = $manifest | ConvertTo-Json -Depth 5
$json | Out-File "target\release\bundle\latest.json" -Encoding utf8
Write-Host "latest.json written:"
Write-Host $json
