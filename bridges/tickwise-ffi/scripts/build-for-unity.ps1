<#
.SYNOPSIS
Builds the release tickwise_ffi.dll and copies it into the Unity package's
native plugin folder for local editor validation.

.DESCRIPTION
Until the release workflow publishes prebuilt binaries onto the upm branch,
this is how the Windows DLL reaches a Unity project on the developer's
machine. The destination is gitignored, so the binary never enters main.

.PARAMETER Destination
Folder to copy the DLL into. Defaults to the package's Windows x86_64
plugin folder.

.EXAMPLE
pwsh bridges/tickwise-ffi/scripts/build-for-unity.ps1
#>
param(
    [string]$Destination = ""
)

$ErrorActionPreference = "Stop"
$ffiDir = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$repoRoot = Split-Path -Parent (Split-Path -Parent $ffiDir)
if ($Destination -eq "") {
    $Destination = Join-Path $repoRoot "bridges/unity/com.cosgunhalil.tickwise/Runtime/Plugins/Windows/x86_64"
}

# Cargo reads .cargo/config.toml from the working directory, not from the
# manifest path, and that file is what links the C runtime statically.
Push-Location $ffiDir
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

$dll = Join-Path $ffiDir "target/release/tickwise_ffi.dll"
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
Copy-Item -Path $dll -Destination (Join-Path $Destination "tickwise_ffi.dll") -Force

$size = (Get-Item $dll).Length
Write-Host "copied tickwise_ffi.dll ($size bytes) to $Destination"
