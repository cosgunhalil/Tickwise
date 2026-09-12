// Build rules for the Tickwise module.
//
// The module is a thin layer over tickwise_ffi, the C ABI of the Rust core.
// Headers and binaries are looked for in two places, in this order:
//
//   1. Source/ThirdParty/TickwiseFfi/, the staged layout a distributed copy
//      of the plugin carries. scripts/stage-ffi.ps1 fills it.
//   2. The sibling crates in the Tickwise repository, for building the
//      plugin straight from a checkout without staging anything.

using System.IO;
using UnrealBuildTool;

public class Tickwise : ModuleRules
{
	public Tickwise(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = ModuleRules.PCHUsageMode.UseExplicitOrSharedPCHs;

		PublicDependencyModuleNames.AddRange(new string[]
		{
			"Core",
			"CoreUObject",
			"Engine",
		});

		PrivateDependencyModuleNames.AddRange(new string[]
		{
			"Projects",
		});

		string ThirdParty = Path.Combine(ModuleDirectory, "..", "ThirdParty", "TickwiseFfi");
		string RepoBridges = Path.GetFullPath(Path.Combine(ModuleDirectory, "..", "..", "..", ".."));

		string IncludeDir;
		string CppIncludeDir;
		string LibDir;
		string BinDir;
		if (File.Exists(Path.Combine(ThirdParty, "include", "tickwise.h")))
		{
			IncludeDir = Path.Combine(ThirdParty, "include");
			CppIncludeDir = IncludeDir;
			LibDir = Path.Combine(ThirdParty, "lib", Target.Platform.ToString());
			BinDir = Path.Combine(ThirdParty, "bin", Target.Platform.ToString());
		}
		else
		{
			IncludeDir = Path.Combine(RepoBridges, "tickwise-ffi", "include");
			CppIncludeDir = Path.Combine(RepoBridges, "tickwise-cpp", "include");
			LibDir = Path.Combine(RepoBridges, "tickwise-ffi", "target", "release");
			BinDir = LibDir;
		}

		PublicIncludePaths.Add(IncludeDir);
		PublicIncludePaths.Add(CppIncludeDir);

		if (Target.Platform == UnrealTargetPlatform.Win64)
		{
			PublicAdditionalLibraries.Add(Path.Combine(LibDir, "tickwise_ffi.dll.lib"));
			// Loaded by hand from the plugin directory in StartupModule, so
			// the import library's symbols must resolve lazily.
			PublicDelayLoadDLLs.Add("tickwise_ffi.dll");
			RuntimeDependencies.Add(Path.Combine(BinDir, "tickwise_ffi.dll"));
		}
		else if (Target.Platform == UnrealTargetPlatform.Mac)
		{
			string Dylib = Path.Combine(LibDir, "libtickwise_ffi.dylib");
			PublicAdditionalLibraries.Add(Dylib);
			RuntimeDependencies.Add(Dylib);
		}
		else if (Target.Platform == UnrealTargetPlatform.Linux)
		{
			string So = Path.Combine(LibDir, "libtickwise_ffi.so");
			PublicAdditionalLibraries.Add(So);
			RuntimeDependencies.Add(So);
		}

		PublicDefinitions.Add("TICKWISE_FFI_BIN_DIR=\"" + BinDir.Replace("\\", "/") + "\"");
	}
}
