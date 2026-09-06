<#
.SYNOPSIS
Writes the Unity .meta for one native tickwise_ffi binary with the plugin
importer settings for its platform.

.DESCRIPTION
Unity decides where a native library is used from the PluginImporter block
in its .meta, not from folder names, so every binary the package ships
needs one. Neither the binaries nor their metas live on main: Unity deletes
a meta whose asset is missing as soon as a project imports the package, so
the release workflow runs this script for every target while assembling the
upm branch, and build-for-unity.ps1 runs it for the local Windows DLL.

.PARAMETER Target
One of: windows-x86_64, macos, linux-x86_64, android-arm64-v8a,
android-armeabi-v7a, android-x86_64, ios.

.PARAMETER PackageDir
The package root. Defaults to the package beside this script's folder.

.EXAMPLE
pwsh bridges/unity/scripts/new-plugin-meta.ps1 windows-x86_64
#>
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("windows-x86_64", "macos", "linux-x86_64", "android-arm64-v8a", "android-armeabi-v7a", "android-x86_64", "ios")]
    [string]$Target,
    [string]$PackageDir = ""
)

$ErrorActionPreference = "Stop"
if ($PackageDir -eq "") {
    $PackageDir = Join-Path (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)) "com.cosgunhalil.tickwise"
}

# Per target: relative binary path inside Runtime/Plugins, which Unity build
# target is enabled, the CPU setting for that target, and whether the editor
# may load it and on which editor OS.
$table = @{
    "windows-x86_64"      = @{ Path = "Windows/x86_64/tickwise_ffi.dll";           Enable = "Win64";        Cpu = "x86_64"; EditorOs = "Windows"; EditorCpu = "x86_64" }
    "macos"               = @{ Path = "macOS/libtickwise_ffi.dylib";               Enable = "OSXUniversal"; Cpu = "AnyCPU"; EditorOs = "OSX";     EditorCpu = "AnyCPU" }
    "linux-x86_64"        = @{ Path = "Linux/x86_64/libtickwise_ffi.so";           Enable = "Linux64";      Cpu = "x86_64"; EditorOs = "Linux";   EditorCpu = "x86_64" }
    "android-arm64-v8a"   = @{ Path = "Android/arm64-v8a/libtickwise_ffi.so";      Enable = "Android";      Cpu = "ARM64";  EditorOs = "";        EditorCpu = "" }
    "android-armeabi-v7a" = @{ Path = "Android/armeabi-v7a/libtickwise_ffi.so";    Enable = "Android";      Cpu = "ARMv7";  EditorOs = "";        EditorCpu = "" }
    "android-x86_64"      = @{ Path = "Android/x86_64/libtickwise_ffi.so";         Enable = "Android";      Cpu = "X86_64"; EditorOs = "";        EditorCpu = "" }
    "ios"                 = @{ Path = "iOS/libtickwise_ffi.a";                     Enable = "iOS";          Cpu = "ARM64";  EditorOs = "";        EditorCpu = "" }
}
$t = $table[$Target]

function Flag([string]$name) {
    if ($t.Enable -eq $name) { return 0 } else { return 1 }
}
function Enabled([string]$name) {
    if ($t.Enable -eq $name) { return 1 } else { return 0 }
}
function CpuFor([string]$name) {
    if ($t.Enable -eq $name) { return $t.Cpu } else { return "None" }
}

$editorEnabled = if ($t.EditorOs -ne "") { 1 } else { 0 }
$excludeEditor = if ($t.EditorOs -ne "") { 0 } else { 1 }
$editorSettings = if ($t.EditorOs -ne "") {
    "        CPU: $($t.EditorCpu)`n        DefaultValueInitialized: true`n        OS: $($t.EditorOs)"
} else {
    "        DefaultValueInitialized: true"
}
$androidSettings = if ($t.Enable -eq "Android") { "        CPU: $($t.Cpu)" } else { "        CPU: ARMv7" }
$iosSettings = if ($t.Enable -eq "iOS") {
    "        AddToEmbeddedBinaries: false`n        CPU: $($t.Cpu)`n        CompileFlags: `n        FrameworkDependencies: "
} else {
    "        AddToEmbeddedBinaries: false`n        CPU: AnyCPU`n        CompileFlags: `n        FrameworkDependencies: "
}

# The GUID is derived from the binary's path, so every release produces the
# same meta and the upm branch history stays free of GUID churn.
$md5 = [System.Security.Cryptography.MD5]::Create()
$hash = $md5.ComputeHash([System.Text.Encoding]::UTF8.GetBytes("com.cosgunhalil.tickwise/Runtime/Plugins/$($t.Path)"))
$guid = ($hash | ForEach-Object { $_.ToString("x2") }) -join ""

$meta = @"
fileFormatVersion: 2
guid: $guid
PluginImporter:
  externalObjects: {}
  serializedVersion: 2
  iconMap: {}
  executionOrder: {}
  defineConstraints: []
  isPreloaded: 0
  isOverridable: 0
  isExplicitlyReferenced: 0
  validateReferences: 1
  platformData:
  - first:
      : Any
    second:
      enabled: 0
      settings:
        Exclude Android: $(Flag "Android")
        Exclude Editor: $excludeEditor
        Exclude Linux64: $(Flag "Linux64")
        Exclude OSXUniversal: $(Flag "OSXUniversal")
        Exclude Win: 1
        Exclude Win64: $(Flag "Win64")
        Exclude iOS: $(Flag "iOS")
  - first:
      Android: Android
    second:
      enabled: $(Enabled "Android")
      settings:
$androidSettings
  - first:
      Any:
    second:
      enabled: 0
      settings: {}
  - first:
      Editor: Editor
    second:
      enabled: $editorEnabled
      settings:
$editorSettings
  - first:
      Standalone: Linux64
    second:
      enabled: $(Enabled "Linux64")
      settings:
        CPU: $(CpuFor "Linux64")
  - first:
      Standalone: OSXUniversal
    second:
      enabled: $(Enabled "OSXUniversal")
      settings:
        CPU: $(CpuFor "OSXUniversal")
  - first:
      Standalone: Win
    second:
      enabled: 0
      settings:
        CPU: None
  - first:
      Standalone: Win64
    second:
      enabled: $(Enabled "Win64")
      settings:
        CPU: $(CpuFor "Win64")
  - first:
      iPhone: iOS
    second:
      enabled: $(Enabled "iOS")
      settings:
$iosSettings
  userData:
  assetBundleName:
  assetBundleVariant:

"@

$binary = Join-Path $PackageDir "Runtime/Plugins/$($t.Path)"
$folder = Split-Path -Parent $binary
New-Item -ItemType Directory -Force -Path $folder | Out-Null
$metaPath = "$binary.meta"
[System.IO.File]::WriteAllText($metaPath, $meta.Replace("`r`n", "`n"), [System.Text.UTF8Encoding]::new($false))
Write-Host "wrote    $metaPath"
