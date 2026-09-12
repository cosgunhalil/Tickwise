<#
.SYNOPSIS
Compiles the plugin against a temporary host project with Unreal's
automation tool, which is how the plugin is verified without opening the
editor.

.DESCRIPTION
Stages the native library first, because the automation tool copies the
plugin out of the repository before building it and the repository
fallback in Tickwise.Build.cs no longer resolves from there. The packaged
plugin lands in bridges/tickwise-unreal/build/Tickwise and is ready to
copy into any project's Plugins folder.

.PARAMETER EngineRoot
The Unreal Engine installation. Defaults to the 4.26 launcher path.

.PARAMETER TargetPlatforms
Platforms to build for. Win64 by default.

.EXAMPLE
pwsh bridges/tickwise-unreal/scripts/build-plugin.ps1
#>
param(
    [string]$EngineRoot = "C:\Program Files\Epic Games\UE_4.26",
    [string]$TargetPlatforms = "Win64"
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$unreal = Split-Path -Parent $here
$plugin = Join-Path $unreal "Tickwise\Tickwise.uplugin"
$package = Join-Path $unreal "build\Tickwise"
# A launcher build ships the automation tool precompiled, and RunUAT.bat
# does nothing beyond changing into this directory before running it, so
# the executable is called directly and the batch file's quoting quirks
# stay out of the way.
$uatDir = Join-Path $EngineRoot "Engine\Binaries\DotNET"
$uat = Join-Path $uatDir "AutomationTool.exe"

if (-not (Test-Path $uat)) {
    throw "AutomationTool.exe not found under $EngineRoot. Pass -EngineRoot."
}

& (Join-Path $here "stage-ffi.ps1")

if (Test-Path $package) {
    Remove-Item -Recurse -Force $package
}

Push-Location $uatDir
try {
    & $uat BuildPlugin "-Plugin=$plugin" "-Package=$package" "-TargetPlatforms=$TargetPlatforms" -Rocket
    if ($LASTEXITCODE -ne 0) {
        throw "BuildPlugin failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}
Write-Host "plugin packaged into $package"
