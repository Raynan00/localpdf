# Build the Microsoft Store package (MSIX) from a release build.
#
#   npx tauri build --no-bundle
#   cargo build -p localpdf-shell --release
#   pwsh scripts/msix-package.ps1 -IdentityName <...> -Publisher "CN=..." -PublisherDisplayName <...>
#
# The identity values come from Partner Center (Product management -> Product
# identity). The Store signs the package itself, so the output is unsigned.
# With -TestCertificate, it is also signed with a throwaway self-signed
# certificate so it can be installed locally for testing.
param(
  [string]$IdentityName = "LocalPDF.Test",
  [string]$Publisher = "CN=LocalPDF Test",
  [string]$PublisherDisplayName = "LocalPDF",
  [switch]$TestCertificate
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$conf = Get-Content "$root\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
$version = "$($conf.version).0"   # MSIX wants four parts, the Store wants the last one to be 0

$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
  Sort-Object { [version]($_.Directory.Parent.Name) } | Select-Object -Last 1
if (-not $sdk) { throw "makeappx.exe not found; install the Windows SDK" }
$bin = $sdk.DirectoryName

$layout = "$root\target\msix\layout"
Remove-Item -Recurse -Force "$root\target\msix" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $layout | Out-Null

Copy-Item "$root\target\release\LocalPDF.exe" $layout
Copy-Item "$root\target\release\localpdf_shell.dll" $layout
Copy-Item -Recurse "$root\src-tauri\engines" "$layout\engines"
Copy-Item -Recurse "$root\src-tauri\windows\msix\Assets" "$layout\Assets"
Copy-Item "$root\LICENSE" "$layout\LICENSE.txt"

(Get-Content "$root\src-tauri\windows\msix\AppxManifest.xml" -Raw).
  Replace("{{IDENTITY_NAME}}", $IdentityName).
  Replace("{{PUBLISHER}}", [System.Security.SecurityElement]::Escape($Publisher)).
  Replace("{{PUBLISHER_DISPLAY_NAME}}", [System.Security.SecurityElement]::Escape($PublisherDisplayName)).
  Replace("{{VERSION}}", $version) |
  Set-Content -Encoding utf8 "$layout\AppxManifest.xml"

$out = "$root\target\msix\LocalPDF_$($conf.version)_x64.msix"
& "$bin\makeappx.exe" pack /o /h SHA256 /d $layout /p $out
if ($LASTEXITCODE) { throw "makeappx failed" }
Write-Host "MSIX: $out ($([math]::Round((Get-Item $out).Length / 1MB, 1)) MB)"

if ($TestCertificate) {
  $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Publisher `
    -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddMonths(3) `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
  $pfx = "$root\target\msix\test-cert.pfx"
  $pw = ConvertTo-SecureString -String "localpdf" -Force -AsPlainText
  Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pw | Out-Null
  Export-Certificate -Cert $cert -FilePath "$root\target\msix\LocalPDF-test.cer" | Out-Null
  $signed = "$root\target\msix\LocalPDF_$($conf.version)_x64_test-signed.msix"
  Copy-Item $out $signed
  & "$bin\signtool.exe" sign /fd SHA256 /f $pfx /p localpdf $signed
  if ($LASTEXITCODE) { throw "signtool failed" }
  Remove-Item $pfx
  Write-Host "Test-signed MSIX: $signed"
}
