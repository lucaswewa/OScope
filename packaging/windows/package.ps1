# Builds OScope's Windows zip (P54, ADR-0033) from a release build of the
# server, and prints its path:
#
#   powershell -File packaging\windows\package.ps1 -Server target\release\oscope-server.exe `
#       [-Notices target\notices] [-Folder target\zip]
#
# -Notices is the third-party notices' folder, as packaging\notices\make.sh
# makes it. -App is the web app, as `npm --prefix web run build` makes it,
# which the server serves at / (W10.4).
param(
    [Parameter(Mandatory = $true)][string]$Server,
    [string]$Notices = "",
    [string]$Folder = "target\zip",
    [string]$App = ""
)
$ErrorActionPreference = "Stop"
# The repository's built app, by default: $PSScriptRoot is not set in param()'s defaults.
if (-not $App) { $App = Join-Path $PSScriptRoot "..\..\web\dist" }
if (-not (Test-Path (Join-Path $App "index.html"))) {
    throw "no web app in $($App): build it first, npm --prefix web ci; npm --prefix web run build"
}
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
# The web app, beside the server, which serves it when its configuration names none.
Copy-Item -Recurse $App (Join-Path $stage "web")
$zip = Join-Path $Folder "$name.zip"
Compress-Archive -Path $stage -DestinationPath $zip -Force
(Resolve-Path $zip).Path
