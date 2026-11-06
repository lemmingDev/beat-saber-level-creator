# Creates a self-signed code signing certificate for Windows builds.
#
# Run this once on a Windows machine (PowerShell, no admin needed):
#
#   .\scripts\generate-self-signed-cert.ps1
#
# It produces:
#   beatmapper-selfsigned.pfx  -> base64 this into the WINDOWS_SELF_SIGN_PFX secret
#   beatmapper-selfsigned.cer  -> distribute this so users can trust your builds
#
# Trust the certificate on a machine with (admin PowerShell):
#
#   certutil -addstore -f Root beatmapper-selfsigned.cer
#
# After that, installers signed with this certificate run without a publisher
# warning on that machine.

param(
    [string]$Subject = "CN=Beatmapper (self-signed)",
    [string]$OutDir = ".",
    [int]$Years = 10
)

$ErrorActionPreference = 'Stop'

Write-Host "Creating self-signed code signing certificate..."
Write-Host "  Subject: $Subject"
Write-Host "  Valid:   $Years years"

$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject $Subject `
    -KeyUsage DigitalSignature `
    -FriendlyName "Beatmapper self-signed code signing" `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -NotAfter (Get-Date).AddYears($Years) `
    -TextExtension @(
        "2.5.29.37={text}1.3.6.1.5.5.7.3.3",   # Extended Key Usage: Code Signing
        "2.5.29.19={text}"                      # Basic Constraints (end entity)
    )

if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}

$pfxPath = Join-Path $OutDir "beatmapper-selfsigned.pfx"
$cerPath = Join-Path $OutDir "beatmapper-selfsigned.cer"

$password = Read-Host -AsSecureString "Choose a password for the .pfx file"

Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $password | Out-Null
Export-Certificate  -Cert $cert -FilePath $cerPath | Out-Null

Write-Host ""
Write-Host "Created:"
Write-Host "  $pfxPath"
Write-Host "  $cerPath"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Base64 the .pfx and store it as repository secret WINDOWS_SELF_SIGN_PFX:"
Write-Host "       [Convert]::ToBase64String([IO.File]::ReadAllBytes('$pfxPath')) | Set-Clipboard"
Write-Host "  2. Store the password as repository secret WINDOWS_SELF_SIGN_PASSWORD"
Write-Host "  3. Users trust the build with (admin PowerShell):"
Write-Host "       certutil -addstore -f Root $cerPath"
