# Trusts (or with -Remove, untrusts) a certificate made by tools\dev_cert.ps1, so Windows will
# install packages signed with it. Run from an admin PowerShell, any folder:
#
#   powershell -ExecutionPolicy Bypass -File "E:\claude code\screen-shifter\tools\trust_dev_cert.ps1"
#   powershell -ExecutionPolicy Bypass -File "E:\claude code\screen-shifter\tools\trust_dev_cert.ps1" -Remove
#
# The certification kit's Store-publisher certificate (remove it again after the kit run):
#   ...\trust_dev_cert.ps1 -Cer store-kit-cert.cer
#   ...\trust_dev_cert.ps1 -Remove -Subject "CN=<Store publisher>"
param([switch]$Remove, [string]$Cer = "dev-cert.cer", [string]$Subject = "CN=ScreenShifterDev")

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Error "Run this from an admin PowerShell."; exit 1 }

$store = "Cert:\LocalMachine\TrustedPeople"
if ($Remove) {
    $found = Get-ChildItem $store | Where-Object Subject -eq $Subject
    $found | Remove-Item
    Write-Host "Removed $(@($found).Count) $Subject certificate(s) from $store."
    exit 0
}

$cer = Join-Path (Split-Path $PSScriptRoot -Parent) $Cer
if (-not (Test-Path $cer)) { Write-Error "$cer not found: run tools\dev_cert.ps1 first."; exit 1 }
$cert = Import-Certificate -FilePath $cer -CertStoreLocation $store
Write-Host "Trusted $($cert.Subject) ($($cert.Thumbprint)) in $store."
