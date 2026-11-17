# Builds OScope's Windows zip (P54, ADR-0033) from a release build of the
# server, and prints its path:
#
#   powershell -File packaging\windows\package.ps1 -Server target\release\oscope-server.exe `
#       [-Notices target\notices] [-Folder target\zip]
#
# -Notices is the third-party notices' folder, as packaging\notices\make.sh
# makes it.
param(
    [Parameter(Mandatory = $true)][string]$Server,
    [string]$Notices = "",
    [string]$Folder = "target\zip"
)
$ErrorActionPreference = "Stop"
$version = ((& $Server --version) -split " ")[1]
$name = "oscope-$version-windows-x86_64"
$stage = Join-Path $Folder $name
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $Server $stage
foreach ($file in "oscope.toml", "install.ps1", "uninstall.ps1", "README.txt", "..\..\LICENSE") {
    Copy-Item (Join-Path $PSScriptRoot $file) $stage
}
if ($Notices) { Copy-Item (Join-Path $Notices "*") $stage }
$zip = Join-Path $Folder "$name.zip"
Compress-Archive -Path $stage -DestinationPath $zip -Force
(Resolve-Path $zip).Path
