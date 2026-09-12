#!/usr/bin/env bash
# Verifies the Unreal plugin layout without an engine: the descriptor
# parses and names the module, the module's build rules and sources are
# where Unreal expects them, and no build output is about to be committed.
set -euo pipefail

plugin="${1:-$(dirname "$0")/../Tickwise}"
cd "$plugin"
failures=0

if ! grep -Eq '"Name"[[:space:]]*:[[:space:]]*"Tickwise"' Tickwise.uplugin; then
    echo "Tickwise.uplugin does not declare the Tickwise module"
    failures=$((failures + 1))
fi
if ! grep -Eq '"EngineVersion"[[:space:]]*:[[:space:]]*"4\.26' Tickwise.uplugin; then
    echo "Tickwise.uplugin does not declare engine version 4.26"
    failures=$((failures + 1))
fi
# Braces and brackets balance, which catches the common hand-edit slips
# without needing a JSON parser on every runner.
opens=$(tr -cd '{[' < Tickwise.uplugin | wc -c)
closes=$(tr -cd '}]' < Tickwise.uplugin | wc -c)
if [ "$opens" -ne "$closes" ]; then
    echo "Tickwise.uplugin has unbalanced brackets"
    failures=$((failures + 1))
fi

for required in \
    Source/Tickwise/Tickwise.Build.cs \
    Source/Tickwise/Public/TickwiseModule.h \
    Source/Tickwise/Public/TickwiseNative.h \
    Source/Tickwise/Public/TickwiseProbe.h \
    Source/Tickwise/Public/TickwiseDump.h \
    Source/Tickwise/Public/TickwiseHashing.h \
    Source/Tickwise/Public/TickwiseRecorderComponent.h \
    Source/Tickwise/Private/TickwiseModule.cpp \
    Source/Tickwise/Private/TickwiseHashing.cpp \
    Source/Tickwise/Private/TickwiseRecorderComponent.cpp; do
    if [ ! -f "$required" ]; then
        echo "missing: $required"
        failures=$((failures + 1))
    fi
done

if ! grep -q 'IMPLEMENT_MODULE(FTickwiseModule, Tickwise)' Source/Tickwise/Private/TickwiseModule.cpp; then
    echo "TickwiseModule.cpp does not implement the module under its declared name"
    failures=$((failures + 1))
fi

for stray in Binaries Intermediate Source/ThirdParty; do
    if git ls-files --error-unmatch "$stray" >/dev/null 2>&1; then
        echo "build output is tracked: $stray"
        failures=$((failures + 1))
    fi
done

if [ "$failures" -ne 0 ]; then
    echo "$failures problem(s) in the Unreal plugin layout"
    exit 1
fi
echo "unreal plugin layout ok"
