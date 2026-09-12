// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#pragma once

#include "CoreMinimal.h"
#include "Modules/ModuleInterface.h"

/**
 * Loads the native tickwise_ffi library when the module starts and
 * refuses to hand out recorders when its ABI does not match the header
 * this plugin was built against.
 */
class FTickwiseModule : public IModuleInterface
{
public:
	virtual void StartupModule() override;
	virtual void ShutdownModule() override;

	/** True when the native library loaded and speaks the expected ABI. */
	static bool IsNativeReady();

private:
	void* NativeHandle = nullptr;
	static bool bNativeReady;
};
