// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#pragma once

#include "CoreMinimal.h"
#include "TickwiseNative.h"

/**
 * A state dump under construction: a flat list of path and value pairs
 * that `tickwise diff` walks field by field. A state writer fills one on
 * the ticks the recorder asks for it, every DumpInterval ticks and on
 * RecordDump.
 *
 * Paths are dotted, with brackets for indices: `Units[2].Cell.X`. Give
 * every array its length under the array's own path with Length, so a
 * shorter list never hides behind a matching tail. Set the same fields
 * the full hash covers: a field the hash sees and the dump does not is a
 * divergence the diff cannot name.
 *
 *   Dump.Int(TEXT("Score"), Score).Int(TEXT("Rng"), RngState).Length(TEXT("Units"), Units.Num());
 *   for (int32 i = 0; i < Units.Num(); ++i)
 *   {
 *       Dump.Vector(FString::Printf(TEXT("Units[%d].Cell"), i), Units[i].Cell);
 *   }
 */
struct TICKWISE_API FTickwiseDump
{
	explicit FTickwiseDump(tickwise::Dump& InInner) : Inner(InInner) {}

	FTickwiseDump& Null(const FString& Path) { Inner.null(ToUtf8(Path)); return *this; }
	FTickwiseDump& Bool(const FString& Path, bool V) { Inner.boolean(ToUtf8(Path), V); return *this; }
	FTickwiseDump& Int(const FString& Path, int64 V) { Inner.i64(ToUtf8(Path), V); return *this; }
	FTickwiseDump& UInt(const FString& Path, uint64 V) { Inner.u64(ToUtf8(Path), V); return *this; }
	FTickwiseDump& Float(const FString& Path, float V) { Inner.f32(ToUtf8(Path), V); return *this; }
	FTickwiseDump& Double(const FString& Path, double V) { Inner.f64(ToUtf8(Path), V); return *this; }
	FTickwiseDump& String(const FString& Path, const FString& V) { Inner.str(ToUtf8(Path), ToUtf8(V)); return *this; }
	FTickwiseDump& Name(const FString& Path, const FName& V) { return String(Path, V.ToString()); }
	FTickwiseDump& Bytes(const FString& Path, const TArray<uint8>& V)
	{
		Inner.bytes(ToUtf8(Path), V.GetData(), V.Num());
		return *this;
	}
	/** An array's length, under the array's own path. */
	FTickwiseDump& Length(const FString& Path, int32 Count) { Inner.len(ToUtf8(Path), static_cast<uint64_t>(Count)); return *this; }

	FTickwiseDump& Vector(const FString& Path, const FVector& V)
	{
		return Float(Path + TEXT(".X"), V.X).Float(Path + TEXT(".Y"), V.Y).Float(Path + TEXT(".Z"), V.Z);
	}
	FTickwiseDump& Vector2D(const FString& Path, const FVector2D& V)
	{
		return Float(Path + TEXT(".X"), V.X).Float(Path + TEXT(".Y"), V.Y);
	}
	FTickwiseDump& IntPoint(const FString& Path, const FIntPoint& V)
	{
		return Int(Path + TEXT(".X"), V.X).Int(Path + TEXT(".Y"), V.Y);
	}
	FTickwiseDump& IntVector(const FString& Path, const FIntVector& V)
	{
		return Int(Path + TEXT(".X"), V.X).Int(Path + TEXT(".Y"), V.Y).Int(Path + TEXT(".Z"), V.Z);
	}
	FTickwiseDump& Rotator(const FString& Path, const FRotator& V)
	{
		return Float(Path + TEXT(".Pitch"), V.Pitch).Float(Path + TEXT(".Yaw"), V.Yaw).Float(Path + TEXT(".Roll"), V.Roll);
	}
	FTickwiseDump& Quat(const FString& Path, const FQuat& V)
	{
		return Float(Path + TEXT(".X"), V.X).Float(Path + TEXT(".Y"), V.Y).Float(Path + TEXT(".Z"), V.Z).Float(Path + TEXT(".W"), V.W);
	}

	/** Entries set so far. */
	int32 Num() const { return static_cast<int32>(Inner.size()); }

private:
	static std::string ToUtf8(const FString& Value)
	{
		FTCHARToUTF8 Utf8(*Value);
		return std::string(Utf8.Get(), Utf8.Length());
	}

	tickwise::Dump& Inner;
};
