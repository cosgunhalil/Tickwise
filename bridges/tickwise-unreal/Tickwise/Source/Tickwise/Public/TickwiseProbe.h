// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#pragma once

#include "CoreMinimal.h"
#include "UObject/Interface.h"
#include "TickwiseProbe.generated.h"

UINTERFACE(BlueprintType, meta = (DisplayName = "Tickwise Probe"))
class TICKWISE_API UTickwiseProbe : public UInterface
{
	GENERATED_BODY()
};

/**
 * The contract between your simulation and Tickwise: two hashes of
 * gameplay state. Implement it on the actor or object that owns the
 * simulation, in C++ or in Blueprint, and hand it to a recorder component.
 *
 * Both hashes are 64 bit values carried as int64. Build them with
 * FTickwiseHasher in C++, or from UTickwiseHashLibrary in Blueprint.
 */
class TICKWISE_API ITickwiseProbe
{
	GENERATED_BODY()

public:
	/**
	 * A cheap digest of desync critical state, asked for on every tick.
	 * Hash the values that would make two machines disagree: entity count,
	 * player states, the random seed, the score. Stay well under one
	 * percent of the tick.
	 */
	UFUNCTION(BlueprintNativeEvent, BlueprintCallable, Category = "Tickwise")
	int64 LightHash() const;

	/**
	 * A hash over all gameplay state, asked for only on the ticks the
	 * recorder keeps a full hash, every 300 by default. Anything left out
	 * of it is a blind spot where a desync can hide.
	 */
	UFUNCTION(BlueprintNativeEvent, BlueprintCallable, Category = "Tickwise")
	int64 FullHash() const;
};
