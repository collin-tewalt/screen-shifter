# Trusts (or with -Remove, untrusts) the dev certificate made by tools\dev_cert.ps1, so Windows
# will install packages signed by CN=ScreenShifterDev. Run from an admin PowerShell, any folder:
#
#   powershell -ExecutionPolicy Bypass -File "E:\claude code\screen-shifter\tools\trust_dev_cert.ps1"
#   powershell -ExecutionPolicy Bypass -File "E:\claude code\screen-shifter\tools\trust_dev_cert.ps1" -Remove
param([switch]$Remove)

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Error "Run this from an admin PowerShell."; exit 1 }

$store = "Cert:\LocalMachine\TrustedPeople"
if ($Remove) {
    $found = Get-ChildItem $store | Where-Object Subject -eq "CN=ScreenShifterDev"
    $found | Remove-Item
    Write-Host "Removed $(@($found).Count) ScreenShifterDev certificate(s) from $store."
    exit 0
}

$cer = Join-Path (Split-Path $PSScriptRoot -Parent) "dev-cert.cer"
if (-not (Test-Path $cer)) { Write-Error "$cer not found: run tools\dev_cert.ps1 first."; exit 1 }
$cert = Import-Certificate -FilePath $cer -CertStoreLocation $store
Write-Host "Trusted $($cert.Subject) ($($cert.Thumbprint)) in $store."
