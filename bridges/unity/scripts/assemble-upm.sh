#!/usr/bin/env bash
# Assembles the release form of the Unity package: the package sources plus
# the native binaries and their plugin importer metas, ready to be committed
# to the upm branch.
#
# Usage: assemble-upm.sh <version> <binaries-dir> <out-dir> [--allow-missing]
#
# <binaries-dir> holds one folder per target, named like the targets of
# new-plugin-meta.ps1, each containing the binary:
#   windows-x86_64/tickwise_ffi.dll      macos/libtickwise_ffi.dylib
#   linux-x86_64/libtickwise_ffi.so      android-arm64-v8a/libtickwise_ffi.so
#   android-armeabi-v7a/libtickwise_ffi.so   android-x86_64/libtickwise_ffi.so
#   ios/libtickwise_ffi.a
# A release requires all seven. --allow-missing skips absent ones, for local
# dry runs with only the Windows build at hand.
set -euo pipefail

version="${1:?version, for example 0.1.0}"
binaries="${2:?binaries dir}"
out="${3:?output dir}"
allow_missing="${4:-}"

scripts="$(cd "$(dirname "$0")" && pwd)"
package="$scripts/../com.cosgunhalil.tickwise"
declared="$(grep -E '^\s*"version"\s*:' "$package/package.json" | sed -E 's/.*"version"\s*:\s*"([^"]+)".*/\1/')"
if [ "$declared" != "$version" ]; then
    echo "package.json declares version $declared, the release asks for $version" >&2
    exit 1
fi

rm -rf "$out"
mkdir -p "$out"
# The package contents as committed: sources, metas, Samples~, documentation.
# .gitkeep files are dropped because every folder is non-empty afterwards.
cp -R "$package/." "$out/"
find "$out" -name .gitkeep -type f -delete

declare -A destination=(
    [windows-x86_64]="Runtime/Plugins/Windows/x86_64/tickwise_ffi.dll"
    [macos]="Runtime/Plugins/macOS/libtickwise_ffi.dylib"
    [linux-x86_64]="Runtime/Plugins/Linux/x86_64/libtickwise_ffi.so"
    [android-arm64-v8a]="Runtime/Plugins/Android/arm64-v8a/libtickwise_ffi.so"
    [android-armeabi-v7a]="Runtime/Plugins/Android/armeabi-v7a/libtickwise_ffi.so"
    [android-x86_64]="Runtime/Plugins/Android/x86_64/libtickwise_ffi.so"
    [ios]="Runtime/Plugins/iOS/libtickwise_ffi.a"
)

if command -v pwsh >/dev/null 2>&1; then
    powershell_cmd=pwsh
else
    powershell_cmd=powershell
fi

placed=0
for target in windows-x86_64 macos linux-x86_64 android-arm64-v8a android-armeabi-v7a android-x86_64 ios; do
    dest="${destination[$target]}"
    source="$binaries/$target/$(basename "$dest")"
    if [ ! -f "$source" ]; then
        if [ "$allow_missing" = "--allow-missing" ]; then
            echo "skipping $target, no binary at $source"
            continue
        fi
        echo "missing binary for $target at $source" >&2
        exit 1
    fi
    mkdir -p "$out/$(dirname "$dest")"
    cp "$source" "$out/$dest"
    "$powershell_cmd" -NoProfile -ExecutionPolicy Bypass -File "$scripts/new-plugin-meta.ps1" "$target" -PackageDir "$out" >/dev/null
    placed=$((placed + 1))
    echo "placed   $dest"
done

bash "$scripts/check-package.sh" "$out"
echo "assembled com.cosgunhalil.tickwise $version with $placed native binaries in $out"
