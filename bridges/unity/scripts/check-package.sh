#!/usr/bin/env bash
# Verifies the Unity package layout without an editor:
#   1. package.json is valid JSON and declares the expected fields.
#   2. Every asset file and folder has a .meta beside it, and every .meta
#      has its asset. Unity regenerates missing metas with new GUIDs, which
#      breaks references for every user of a git URL package.
# Folders ending in ~ and hidden entries are ignored by Unity and need no meta.
set -euo pipefail

package_dir="${1:-$(dirname "$0")/../com.cosgunhalil.tickwise}"
cd "$package_dir"
failures=0

for field in name version displayName unity license; do
    if ! grep -Eq "^[[:space:]]*\"$field\"[[:space:]]*:" package.json; then
        echo "package.json is missing the field: $field"
        failures=$((failures + 1))
    fi
done
if ! grep -Eq '^[[:space:]]*"unity"[[:space:]]*:[[:space:]]*"2022\.3"' package.json; then
    echo "package.json must declare \"unity\": \"2022.3\", the supported minimum"
    failures=$((failures + 1))
fi

# Native binaries are gitignored on main and arrive on the upm branch, so a
# binary present locally without a meta, or a committed meta without its
# binary in CI, are both expected.
is_native_binary() {
    case "$1" in
        *.dll|*.so|*.dylib|*.a|*.bundle) return 0 ;;
        *) return 1 ;;
    esac
}

while IFS= read -r -d '' entry; do
    rel="${entry#./}"
    case "$rel" in
        *.meta) ;;
        *)
            if is_native_binary "$rel"; then
                continue
            fi
            if [ ! -f "$rel.meta" ]; then
                echo "missing meta: $rel"
                failures=$((failures + 1))
            fi
            ;;
    esac
done < <(find . -mindepth 1 \( -name '*~' -o -name '.*' \) -prune -o -print0)

while IFS= read -r -d '' meta; do
    asset="${meta%.meta}"
    if [ ! -e "$asset" ] && ! is_native_binary "$asset"; then
        echo "orphan meta: ${meta#./}"
        failures=$((failures + 1))
    fi
done < <(find . -name '*.meta' -print0)

# A native binary that is present, locally or on the upm branch, must carry a
# plugin importer meta, or Unity would import it with default settings and
# try to load it on every platform at once. Absent binaries have no meta on
# main, because Unity deletes metas of missing assets on import.
while IFS= read -r -d '' binary; do
    rel="${binary#./}"
    if [ ! -f "$rel.meta" ]; then
        echo "native binary without a plugin meta: $rel, run scripts/new-plugin-meta.ps1"
        failures=$((failures + 1))
    elif ! grep -q "^PluginImporter:" "$rel.meta"; then
        echo "not a plugin importer meta: $rel.meta"
        failures=$((failures + 1))
    fi
done < <(find ./Runtime/Plugins -type f \( -name '*.dll' -o -name '*.so' -o -name '*.dylib' -o -name '*.a' \) -print0 2>/dev/null)

# The seven platform folders stay in git so a release only adds files.
for folder in Windows/x86_64 macOS Linux/x86_64 Android/arm64-v8a Android/armeabi-v7a Android/x86_64 iOS; do
    if [ ! -f "Runtime/Plugins/$folder.meta" ]; then
        echo "missing plugin folder meta: Runtime/Plugins/$folder.meta"
        failures=$((failures + 1))
    fi
done

if [ "$failures" -ne 0 ]; then
    echo "$failures problem(s) in the Unity package layout"
    exit 1
fi
echo "unity package layout ok"
