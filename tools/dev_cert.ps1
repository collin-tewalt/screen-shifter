# One-time: a self-signed certificate for sideloading dev packages.
# Its subject must equal the manifest Publisher (tools/package.py default: CN=ScreenShifterDev).
#
#   powershell -ExecutionPolicy Bypass -File tools\dev_cert.ps1
#
# For the certification kit's copy of the Store package, a second one with the Store publisher,
# in separate files (the dev certificate is never overwritten):
#
#   powershell -ExecutionPolicy Bypass -File tools\dev_cert.ps1 -Subject "CN=<Store publisher>" -Out store-kit-cert
#
# Then trust it (admin, once) so Windows will install packages signed with it:
#   Import-Certificate -FilePath dev-cert.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople

param([string]$Subject = "CN=ScreenShifterDev", [string]$Out = "dev-cert")

$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Subject `
    -CertStoreLocation Cert:\CurrentUser\My -KeyUsage DigitalSignature `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
$password = ConvertTo-SecureString -String "dev" -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath "$Out.pfx" -Password $password | Out-Null
Export-Certificate -Cert $cert -FilePath "$Out.cer" | Out-Null
Write-Host "Wrote $Out.pfx (password: dev) and $Out.cer for $Subject. Package with:"
if ($Out -eq "dev-cert") {
    Write-Host "  python tools/package.py --sign dev-cert.pfx --password dev"
} else {
    Write-Host "  python tools/package.py --store --sign $Out.pfx --password dev"
}
