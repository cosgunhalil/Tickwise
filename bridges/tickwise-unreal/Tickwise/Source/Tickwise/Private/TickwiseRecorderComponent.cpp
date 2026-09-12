// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#include "TickwiseRecorderComponent.h"
#include "TickwiseModule.h"

#include "GameFramework/Actor.h"
#include "Misc/DateTime.h"
#include "Misc/Paths.h"

namespace
{
	/** Adapts a UObject implementing the probe interface to the C++ wrapper. */
	struct FProbeAdapter : tickwise::Probe
	{
		explicit FProbeAdapter(UObject* InObject) : Object(InObject) {}

		uint64_t light_hash() const override
		{
			return static_cast<uint64_t>(ITickwiseProbe::Execute_LightHash(Object));
		}

		uint64_t full_hash() const override
		{
			return static_cast<uint64_t>(ITickwiseProbe::Execute_FullHash(Object));
		}

		UObject* Object;
	};

	std::string ToUtf8(const FString& Value)
	{
		FTCHARToUTF8 Utf8(*Value);
		return std::string(Utf8.Get(), Utf8.Length());
	}
}

UTickwiseRecorderComponent::UTickwiseRecorderComponent()
{
	PrimaryComponentTick.bCanEverTick = true;
	// Late in the frame, so a game stepping from its own tick has already
	// produced the state this component records.
	PrimaryComponentTick.TickGroup = TG_PostUpdateWork;
}

bool UTickwiseRecorderComponent::StartRecording(const FString& Path)
{
	StopRecording();
	LastError.Empty();

	if (!FTickwiseModule::IsNativeReady())
	{
		Fail(TEXT("the Tickwise native library is not loaded, see the log for the reason"));
		return false;
	}

	RecordingPath = FPaths::IsRelative(Path) ? FPaths::Combine(FPaths::ProjectSavedDir(), Path) : Path;
	RecordingPath = FPaths::ConvertRelativePathToFull(RecordingPath);

	tickwise::Config Config;
	Config.game_id = ToUtf8(GameId);
	Config.build_hash = ToUtf8(BuildHash);
	Config.platform = ToUtf8(FString(FPlatformProperties::IniPlatformName()));
	Config.tick_rate = static_cast<uint32_t>(FMath::Max(TickRate, 0));
	Config.rng_seed = static_cast<uint64_t>(RngSeed);
	Config.created_at = static_cast<uint64_t>(FDateTime::UtcNow().ToUnixTimestamp());
	Config.full_hash_interval = static_cast<uint32_t>(FMath::Max(FullHashInterval, 0));
	Config.input_format_id = static_cast<uint64_t>(InputFormatId);
	// FTickwiseHasher and the Blueprint library both produce xxh3.
	Config.hash_algo_id = tickwise::hash_algo::kXxh3;

	if (Recorder.open(ToUtf8(RecordingPath), Config) != tickwise::Status::Ok)
	{
		Fail(FString::Printf(TEXT("cannot record to %s: %s"), *RecordingPath, UTF8_TO_TCHAR(Recorder.last_error().c_str())));
		return false;
	}
	NextTick = 0;
	UE_LOG(LogTickwise, Log, TEXT("recording to %s"), *RecordingPath);
	return true;
}

void UTickwiseRecorderComponent::StopRecording()
{
	if (!Recorder.is_recording())
	{
		return;
	}
	if (Recorder.finish() != tickwise::Status::Ok)
	{
		LastError = UTF8_TO_TCHAR(Recorder.last_error().c_str());
		UE_LOG(LogTickwise, Error, TEXT("%s"), *LastError);
	}
	Recorder.destroy();
	UE_LOG(LogTickwise, Log, TEXT("finished %llu ticks, saved %s"), NextTick, *RecordingPath);
}

void UTickwiseRecorderComponent::SetInputs(const TArray<uint8>& Inputs)
{
	PendingInputs = Inputs;
}

bool UTickwiseRecorderComponent::RecordTick()
{
	if (!Recorder.is_recording())
	{
		return false;
	}
	UObject* ProbeObject = ResolveProbe();
	if (!ProbeObject)
	{
		Fail(TEXT("no probe: set the Probe property or implement Tickwise Probe on the owning actor"));
		return false;
	}

	FProbeAdapter Adapter(ProbeObject);
	tickwise::Status Status = Recorder.record_tick(NextTick, PendingInputs.GetData(), PendingInputs.Num(), Adapter);
	if (Status != tickwise::Status::Ok)
	{
		Fail(UTF8_TO_TCHAR(Recorder.last_error().c_str()));
		return false;
	}
	++NextTick;
	return true;
}

void UTickwiseRecorderComponent::RecordMarker(const FString& Label)
{
	if (!Recorder.is_recording())
	{
		return;
	}
	uint64 Tick = NextTick == 0 ? 0 : NextTick - 1;
	if (Recorder.record_marker(Tick, ToUtf8(Label)) != tickwise::Status::Ok)
	{
		Fail(UTF8_TO_TCHAR(Recorder.last_error().c_str()));
	}
}

bool UTickwiseRecorderComponent::IsRecording() const
{
	return Recorder.is_recording();
}

int64 UTickwiseRecorderComponent::GetTick() const
{
	return static_cast<int64>(NextTick);
}

FString UTickwiseRecorderComponent::GetLastError() const
{
	return LastError;
}

FString UTickwiseRecorderComponent::GetRecordingPath() const
{
	return RecordingPath;
}

void UTickwiseRecorderComponent::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);
	if (bRecordEveryComponentTick && Recorder.is_recording())
	{
		RecordTick();
	}
}

void UTickwiseRecorderComponent::EndPlay(const EEndPlayReason::Type EndPlayReason)
{
	// Ending play ends the session, so stopping the editor still leaves a
	// readable recording.
	StopRecording();
	Super::EndPlay(EndPlayReason);
}

UObject* UTickwiseRecorderComponent::ResolveProbe() const
{
	if (UObject* Explicit = Probe.GetObject())
	{
		if (Explicit->GetClass()->ImplementsInterface(UTickwiseProbe::StaticClass()))
		{
			return Explicit;
		}
	}
	AActor* Owner = GetOwner();
	if (Owner && Owner->GetClass()->ImplementsInterface(UTickwiseProbe::StaticClass()))
	{
		return Owner;
	}
	return nullptr;
}

void UTickwiseRecorderComponent::Fail(const FString& Message)
{
	LastError = Message;
	UE_LOG(LogTickwise, Error, TEXT("tickwise: %s"), *Message);
	// The file is already unusable, and continuing would only produce the
	// same error on every tick.
	Recorder.destroy();
}
