<#
.SYNOPSIS
Writes a Unity .meta file with a fresh GUID for an asset that lacks one.

.DESCRIPTION
Unity generates .meta files when it imports a folder, but a package
installed by git URL never gets that chance before its GUIDs matter, so
this repository commits metas for every asset. Run this after adding a
file or folder to the package. Existing metas are never overwritten.

.PARAMETER Path
One or more asset paths, files or folders, relative to the current
directory.

.EXAMPLE
pwsh bridges/unity/scripts/new-meta.ps1 bridges/unity/com.cosgunhalil.tickwise/Runtime/Native.cs
#>
param(
    [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
    [string[]]$Path
)

$ErrorActionPreference = "Stop"

foreach ($asset in $Path) {
    $meta = "$asset.meta"
    if (Test-Path $meta) {
        Write-Host "exists   $meta"
        continue
    }
    $guid = [guid]::NewGuid().ToString("N")
    $body = if (Test-Path $asset -PathType Container) {
        "fileFormatVersion: 2`nguid: $guid`nfolderAsset: yes`nDefaultImporter:`n  externalObjects: {}`n  userData: `n  assetBundleName: `n  assetBundleVariant: `n"
    }
    elseif ($asset -like "*.asmdef") {
        "fileFormatVersion: 2`nguid: $guid`nAssemblyDefinitionImporter:`n  externalObjects: {}`n  userData: `n  assetBundleName: `n  assetBundleVariant: `n"
    }
    elseif ($asset -like "*.cs") {
        "fileFormatVersion: 2`nguid: $guid`nMonoImporter:`n  externalObjects: {}`n  serializedVersion: 2`n  defaultReferences: []`n  executionOrder: 0`n  icon: {instanceID: 0}`n  userData: `n  assetBundleName: `n  assetBundleVariant: `n"
    }
    else {
        "fileFormatVersion: 2`nguid: $guid`nTextScriptImporter:`n  externalObjects: {}`n  userData: `n  assetBundleName: `n  assetBundleVariant: `n"
    }
    [System.IO.File]::WriteAllText((Join-Path (Get-Location) $meta), $body, [System.Text.UTF8Encoding]::new($false))
    Write-Host "created  $meta"
}
