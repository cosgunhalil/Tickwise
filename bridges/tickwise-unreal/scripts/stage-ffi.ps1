<#
.SYNOPSIS
Builds tickwise_ffi in release and stages its headers and Windows binaries
into the plugin's ThirdParty folder, producing a plugin folder that can be
copied into any Unreal project on its own.

.DESCRIPTION
The plugin builds straight from a repository checkout without this step,
because Tickwise.Build.cs falls back to the sibling crates. Staging is for
handing the plugin to a project outside the repository, and for the
release archives. The staged folder is gitignored.

.EXAMPLE
pwsh bridges/tickwise-unreal/scripts/stage-ffi.ps1
#>
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$bridges = Split-Path -Parent (Split-Path -Parent $here)
$ffi = Join-Path $bridges "tickwise-ffi"
$cpp = Join-Path $bridges "tickwise-cpp"
$thirdParty = Join-Path $bridges "tickwise-unreal/Tickwise/Source/ThirdParty/TickwiseFfi"

# Cargo reads the crate's .cargo/config.toml, which links the C runtime
# statically, only from the working directory.
Push-Location $ffi
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path (Join-Path $thirdParty "include/tickwise") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $thirdParty "lib/Win64") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $thirdParty "bin/Win64") | Out-Null

Copy-Item (Join-Path $ffi "include/tickwise.h") (Join-Path $thirdParty "include/") -Force
Copy-Item (Join-Path $cpp "include/tickwise/tickwise.hpp") (Join-Path $thirdParty "include/tickwise/") -Force
Copy-Item (Join-Path $ffi "target/release/tickwise_ffi.dll.lib") (Join-Path $thirdParty "lib/Win64/") -Force
Copy-Item (Join-Path $ffi "target/release/tickwise_ffi.dll") (Join-Path $thirdParty "bin/Win64/") -Force
Copy-Item (Join-Path $ffi "LICENSE-MIT"), (Join-Path $ffi "LICENSE-APACHE") $thirdParty -Force

Write-Host "staged tickwise_ffi into $thirdParty"
