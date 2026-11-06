# Signs a Windows binary with a self-signed certificate.
#
# Invoked by Tauri via `bundle.windows.signCommand`, so it receives the file to
# sign as its single argument.
#
# Environment variables (set by the workflow):
#   WINDOWS_SELF_SIGN_PFX_PATH  path to the .pfx file on disk
#   WINDOWS_SELF_SIGN_PASSWORD  password for the .pfx
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$FilePath
)

$ErrorActionPreference = 'Stop'

# signtool ships with the Windows SDK; find the newest x64 copy.
$signtool = Get-ChildItem -Path "C:\Program Files (x86)\Windows Kits\10\bin" -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like '*\x64\signtool.exe' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1

if (-not $signtool) {
    # Fall back to whatever is on PATH.
    $signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
}

if (-not $signtool) {
    throw "signtool.exe not found; the Windows SDK is required to sign."
}

$signtoolPath = if ($signtool.FullName) { $signtool.FullName } else { $signtool.Source }

Write-Host "Signing $FilePath"
Write-Host "  with $signtoolPath"

& $signtoolPath sign `
    /f $env:WINDOWS_SELF_SIGN_PFX_PATH `
    /p $env:WINDOWS_SELF_SIGN_PASSWORD `
    /fd sha256 `
    /td sha256 `
    /tr http://timestamp.digicert.com `
    $FilePath

if ($LASTEXITCODE -ne 0) {
    throw "signtool sign failed with exit code $LASTEXITCODE"
}

& $signtoolPath verify /pa /v $FilePath
if ($LASTEXITCODE -ne 0) {
    throw "signature verification failed with exit code $LASTEXITCODE"
}

Write-Host "Signed and verified: $FilePath"
