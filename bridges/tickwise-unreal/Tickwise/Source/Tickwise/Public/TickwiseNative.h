// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#pragma once

#include "CoreMinimal.h"

// The shared C++ wrapper over the C ABI. Third party guards keep the
// engine's warning settings from applying to standard library headers.
THIRD_PARTY_INCLUDES_START
#include "tickwise/tickwise.hpp"
THIRD_PARTY_INCLUDES_END

DECLARE_LOG_CATEGORY_EXTERN(LogTickwise, Log, All);
