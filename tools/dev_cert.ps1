# One-time: a self-signed certificate for sideloading dev packages.
# Its subject must equal the manifest Publisher (tools/package.py default: CN=ScreenShifterDev).
#
#   powershell -ExecutionPolicy Bypass -File tools\dev_cert.ps1
#
# Then trust it (admin, once) so Windows will install packages signed with it:
#   Import-Certificate -FilePath dev-cert.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople

$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=ScreenShifterDev" `
    -CertStoreLocation Cert:\CurrentUser\My -KeyUsage DigitalSignature `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
$password = ConvertTo-SecureString -String "dev" -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath dev-cert.pfx -Password $password | Out-Null
Export-Certificate -Cert $cert -FilePath dev-cert.cer | Out-Null
Write-Host "Wrote dev-cert.pfx (password: dev) and dev-cert.cer. Package with:"
Write-Host "  python tools/package.py --sign dev-cert.pfx --password dev"
