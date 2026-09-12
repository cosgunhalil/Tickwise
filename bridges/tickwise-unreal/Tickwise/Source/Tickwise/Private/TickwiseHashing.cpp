// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#include "TickwiseHashing.h"

int64 UTickwiseHashLibrary::HashBytes(const TArray<uint8>& Bytes)
{
	return static_cast<int64>(tickwise::xxh3_64(Bytes.GetData(), Bytes.Num()));
}

int64 UTickwiseHashLibrary::HashInts(const TArray<int64>& Values)
{
	FTickwiseHasher Hasher;
	for (int64 Value : Values)
	{
		Hasher.Add(Value);
	}
	return Hasher.FinishSigned();
}

int64 UTickwiseHashLibrary::HashFloats(const TArray<float>& Values)
{
	FTickwiseHasher Hasher;
	for (float Value : Values)
	{
		Hasher.Add(Value);
	}
	return Hasher.FinishSigned();
}

int64 UTickwiseHashLibrary::HashString(const FString& Value)
{
	FTickwiseHasher Hasher;
	return Hasher.Add(Value).FinishSigned();
}

int64 UTickwiseHashLibrary::CombineHashes(const TArray<int64>& Hashes)
{
	return HashInts(Hashes);
}
