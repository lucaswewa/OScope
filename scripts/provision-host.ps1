#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$BaselinePath,
    [Parameter(Mandatory)][string]$BundleDirectory,
    [Parameter(Mandatory)][string]$InstallRoot,
    [Parameter(Mandatory)][string]$InstrumentSerial,
    [Parameter(Mandatory)][string]$PcSerial,
    [ValidateSet('Plan', 'Apply')][string]$Mode = 'Plan',
    [int]$Port = 8210,
    [pscredential]$HostCredential,
    [switch]$Engineering
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'provision-host.psm1') -Force
$plan = Get-OScopePlan $BaselinePath $BundleDirectory $InstallRoot $InstrumentSerial $PcSerial $Port
if ($Mode -eq 'Plan') { $plan | ConvertTo-Json -Depth 20 }
else {
    if (!$HostCredential) { throw 'Apply requires credentials for the dedicated Windows WSL owner' }
    Invoke-OScopeProvision $plan $HostCredential -Engineering:$Engineering | ConvertTo-Json -Depth 5
}
