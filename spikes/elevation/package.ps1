# Builds and signs the spike's test package (identity ScreenShifter.Spike, signed with the dev
# certificate the owner already trusts). Run from any PowerShell:
#
#   powershell -ExecutionPolicy Bypass -File "E:\claude code\screen-shifter\spikes\elevation\package.ps1"
#
# Then install it (the package declares the restricted allowElevation capability, so install
# from an ADMIN PowerShell):  Add-AppxPackage "<repo>\spikes\elevation\ss-spike.msix"
# Remove it afterwards:       Get-AppxPackage ScreenShifter.Spike | Remove-AppxPackage
# -NoCap builds ss-spike-nocap.msix: identity ScreenShifter.SpikeNoCap, WITHOUT allowElevation.
param([switch]$NoCap)
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
$repo = Split-Path (Split-Path $here -Parent) -Parent
$sdk = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin\10.*\x64" -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName "makeappx.exe") } |
    Sort-Object { [version]$_.Parent.Name } | Select-Object -Last 1

Push-Location $here
try { cargo build --release; if ($LASTEXITCODE) { throw "cargo build failed" } } finally { Pop-Location }

$stage = Join-Path $here "stage"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory "$stage\Assets" | Out-Null
Copy-Item "$here\target\release\ss-spike.exe" $stage
$manifest = Get-Content "$here\AppxManifest.xml" -Raw
if ($NoCap) {
    $manifest = $manifest.Replace('Name="ScreenShifter.Spike"', 'Name="ScreenShifter.SpikeNoCap"')
    $manifest = $manifest.Replace('    <rescap:Capability Name="allowElevation" />' + "`r`n", '').Replace('    <rescap:Capability Name="allowElevation" />' + "`n", '')
    $manifest = $manifest.Replace('Alias="ss-spike.exe"', 'Alias="ss-spike-nocap.exe"')
    $manifest = $manifest.Replace('Screen Shifter elevation spike', 'Screen Shifter elevation spike (no capability)')
    if ($manifest.Contains('allowElevation')) { throw "allowElevation still in the NoCap manifest" }
}
[IO.File]::WriteAllText("$stage\AppxManifest.xml", $manifest)
# unqualified logo names, so no resources.pri is needed
Copy-Item "$repo\packaging\Assets\Square150x150Logo.scale-100.png" "$stage\Assets\Square150x150Logo.png"
Copy-Item "$repo\packaging\Assets\Square44x44Logo.scale-100.png" "$stage\Assets\Square44x44Logo.png"
Copy-Item "$repo\packaging\Assets\StoreLogo.png" "$stage\Assets\StoreLogo.png"

$msix = Join-Path $here $(if ($NoCap) { "ss-spike-nocap.msix" } else { "ss-spike.msix" })
& "$($sdk.FullName)\makeappx.exe" pack /d $stage /p $msix /o | Out-Null
if ($LASTEXITCODE) { throw "makeappx failed" }
& "$($sdk.FullName)\signtool.exe" sign /fd SHA256 /f "$repo\dev-cert.pfx" /p dev $msix | Out-Null
if ($LASTEXITCODE) { throw "signtool failed (is dev-cert.pfx there?)" }
Write-Host "Built $msix"
