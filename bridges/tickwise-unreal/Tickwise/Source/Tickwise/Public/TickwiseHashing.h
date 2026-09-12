// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#pragma once

#include "CoreMinimal.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "TickwiseNative.h"
#include "TickwiseHashing.generated.h"

/**
 * Builds a hash field by field, in a fixed byte layout, so a probe never
 * has to think about padding or byte order. Keep one per probe and reuse
 * it: Reset keeps the buffer's allocation.
 *
 *   Hasher.Reset().Add(Score).Add(RngState).Add(Position).Finish()
 */
struct TICKWISE_API FTickwiseHasher
{
	FTickwiseHasher& Reset() { Inner.reset(); return *this; }

	FTickwiseHasher& Add(bool V) { Inner.boolean(V); return *this; }
	FTickwiseHasher& Add(uint8 V) { Inner.u8(V); return *this; }
	FTickwiseHasher& Add(int32 V) { Inner.i32(V); return *this; }
	FTickwiseHasher& Add(uint32 V) { Inner.u32(V); return *this; }
	FTickwiseHasher& Add(int64 V) { Inner.i64(V); return *this; }
	FTickwiseHasher& Add(uint64 V) { Inner.u64(V); return *this; }
	FTickwiseHasher& Add(float V) { Inner.f32(V); return *this; }
	FTickwiseHasher& Add(double V) { Inner.f64(V); return *this; }
	FTickwiseHasher& Add(const FVector& V) { Inner.f32(V.X).f32(V.Y).f32(V.Z); return *this; }
	FTickwiseHasher& Add(const FVector2D& V) { Inner.f32(V.X).f32(V.Y); return *this; }
	FTickwiseHasher& Add(const FIntPoint& V) { Inner.i32(V.X).i32(V.Y); return *this; }
	FTickwiseHasher& Add(const FIntVector& V) { Inner.i32(V.X).i32(V.Y).i32(V.Z); return *this; }
	FTickwiseHasher& Add(const FRotator& V) { Inner.f32(V.Pitch).f32(V.Yaw).f32(V.Roll); return *this; }
	FTickwiseHasher& Add(const FQuat& V) { Inner.f32(V.X).f32(V.Y).f32(V.Z).f32(V.W); return *this; }
	FTickwiseHasher& Add(const FString& V)
	{
		FTCHARToUTF8 Utf8(*V);
		Inner.u32(static_cast<uint32_t>(Utf8.Length()));
		Inner.raw(Utf8.Get(), Utf8.Length());
		return *this;
	}
	FTickwiseHasher& Add(const FName& V) { return Add(V.ToString()); }
	FTickwiseHasher& AddBytes(const TArray<uint8>& V)
	{
		Inner.u32(static_cast<uint32_t>(V.Num()));
		Inner.raw(V.GetData(), V.Num());
		return *this;
	}

	/** The xxh3 of everything added since the last Reset. */
	uint64 Finish() const { return Inner.finish(); }

	/** The same value as int64, the type Blueprint interfaces carry. */
	int64 FinishSigned() const { return static_cast<int64>(Inner.finish()); }

	/** The bytes themselves, for a snapshot. */
	TArray<uint8> Bytes() const
	{
		const std::vector<uint8_t>& B = Inner.bytes();
		TArray<uint8> Out;
		Out.Append(B.data(), static_cast<int32>(B.size()));
		return Out;
	}

private:
	tickwise::Hasher Inner;
};

/**
 * Hashing for Blueprint probes. Each call hashes one buffer with xxh3;
 * combine several with CombineHashes in a fixed order.
 */
UCLASS()
class TICKWISE_API UTickwiseHashLibrary : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

public:
	/** xxh3 of a byte array. */
	UFUNCTION(BlueprintPure, Category = "Tickwise|Hashing")
	static int64 HashBytes(const TArray<uint8>& Bytes);

	/** xxh3 of a list of integers, in order. */
	UFUNCTION(BlueprintPure, Category = "Tickwise|Hashing")
	static int64 HashInts(const TArray<int64>& Values);

	/** xxh3 of a list of floats by bit pattern, in order. */
	UFUNCTION(BlueprintPure, Category = "Tickwise|Hashing")
	static int64 HashFloats(const TArray<float>& Values);

	/** xxh3 of a string's UTF-8 bytes. */
	UFUNCTION(BlueprintPure, Category = "Tickwise|Hashing")
	static int64 HashString(const FString& Value);

	/**
	 * Folds several hashes into one. Order matters, which is what you
	 * want: swapping two fields is a difference.
	 */
	UFUNCTION(BlueprintPure, Category = "Tickwise|Hashing")
	static int64 CombineHashes(const TArray<int64>& Hashes);
};
