// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#include "TickwiseModule.h"
#include "TickwiseNative.h"

#include "HAL/PlatformProcess.h"
#include "Interfaces/IPluginManager.h"
#include "Misc/Paths.h"

DEFINE_LOG_CATEGORY(LogTickwise);

bool FTickwiseModule::bNativeReady = false;

void FTickwiseModule::StartupModule()
{
#if PLATFORM_WINDOWS
	// The import library is delay loaded, so the DLL is resolved from the
	// plugin's own binaries folder rather than from wherever the process
	// happens to be looking.
	FString BaseDir;
	TSharedPtr<IPlugin> Plugin = IPluginManager::Get().FindPlugin(TEXT("Tickwise"));
	if (Plugin.IsValid())
	{
		BaseDir = Plugin->GetBaseDir();
	}
	TArray<FString> Candidates;
	Candidates.Add(FPaths::Combine(BaseDir, TEXT("Source/ThirdParty/TickwiseFfi/bin/Win64/tickwise_ffi.dll")));
	Candidates.Add(FPaths::Combine(BaseDir, TEXT("Binaries/Win64/tickwise_ffi.dll")));
	Candidates.Add(TEXT(TICKWISE_FFI_BIN_DIR "/tickwise_ffi.dll"));

	for (const FString& Candidate : Candidates)
	{
		if (FPaths::FileExists(Candidate))
		{
			NativeHandle = FPlatformProcess::GetDllHandle(*Candidate);
			if (NativeHandle)
			{
				UE_LOG(LogTickwise, Log, TEXT("loaded %s"), *Candidate);
				break;
			}
		}
	}
	if (!NativeHandle)
	{
		UE_LOG(LogTickwise, Error, TEXT("tickwise_ffi.dll was not found. Run scripts/stage-ffi.ps1 or build the tickwise-ffi crate."));
		return;
	}
#endif

	if (!tickwise::abi_matches())
	{
		UE_LOG(LogTickwise, Error,
			TEXT("tickwise_ffi reports ABI version %u, this plugin expects %u. Update both together."),
			tickwise_ffi_abi_version(), tickwise::kExpectedAbiVersion);
		return;
	}

	bNativeReady = true;
	UE_LOG(LogTickwise, Log, TEXT("Tickwise native library %s ready"), ANSI_TO_TCHAR(tickwise::native_version()));
}

void FTickwiseModule::ShutdownModule()
{
	bNativeReady = false;
	if (NativeHandle)
	{
		FPlatformProcess::FreeDllHandle(NativeHandle);
		NativeHandle = nullptr;
	}
}

bool FTickwiseModule::IsNativeReady()
{
	return bNativeReady;
}

IMPLEMENT_MODULE(FTickwiseModule, Tickwise)
