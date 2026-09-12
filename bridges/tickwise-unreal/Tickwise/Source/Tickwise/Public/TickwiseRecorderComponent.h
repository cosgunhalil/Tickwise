// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "TickwiseNative.h"
#include "TickwiseProbe.h"
#include "TickwiseRecorderComponent.generated.h"

/**
 * Records one .rec file per session, one tick per call to RecordTick.
 *
 * Add the component to an actor, point it at a probe, call StartRecording,
 * then call RecordTick from your fixed simulation step after the step has
 * run. The component never decides what a tick is: Unreal's frame is a
 * variable timestep, and a recording is only meaningful when every tick
 * is a simulation tick. For a game that steps in TickComponent at a fixed
 * frame rate, bRecordEveryComponentTick does the call for you.
 *
 * Two recordings of the same match then go to the command line tool:
 *
 *   tickwise compare clean.rec chaotic.rec
 */
UCLASS(ClassGroup = (Tickwise), meta = (BlueprintSpawnableComponent, DisplayName = "Tickwise Recorder"))
class TICKWISE_API UTickwiseRecorderComponent : public UActorComponent
{
	GENERATED_BODY()

public:
	UTickwiseRecorderComponent();

	/** Identifier of the game, written into the recording header. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Session")
	FString GameId;

	/** Build identifier, for example a changelist or version string. Comparing recordings from different builds is a warning, not an error. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Session")
	FString BuildHash;

	/** Simulation rate in ticks per second. Metadata only. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Session", meta = (ClampMin = "0"))
	int32 TickRate = 60;

	/** The seed the simulation started from. Two recordings with different seeds were never going to agree, and compare says so. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Session")
	int64 RngSeed = 0;

	/** How often a full hash is recorded. Zero disables full hashes, which leaves the light hash blind spot uncovered. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Recording", meta = (ClampMin = "0"))
	int32 FullHashInterval = 300;

	/** Your identifier for the input encoding. Change it whenever the bytes change meaning, so a later replay refuses a recording made with the old encoding. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Recording")
	int64 InputFormatId = 0;

	/**
	 * Records a tick from every TickComponent call. Correct only when the
	 * game steps its simulation once per component tick at a fixed frame
	 * rate. Off by default; call RecordTick from your own fixed step instead.
	 */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Recording")
	bool bRecordEveryComponentTick = false;

	/**
	 * The object that hashes your state. When unset, the owning actor is
	 * used if it implements the Tickwise Probe interface.
	 */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Tickwise|Recording")
	TScriptInterface<ITickwiseProbe> Probe;

	/**
	 * Opens a recording. A relative path is resolved under the project's
	 * Saved directory. Returns false on failure; GetLastError says why.
	 */
	UFUNCTION(BlueprintCallable, Category = "Tickwise")
	bool StartRecording(const FString& Path);

	/** Flushes and closes the recording. Ending play does this too. */
	UFUNCTION(BlueprintCallable, Category = "Tickwise")
	void StopRecording();

	/** The input bytes for the current tick. Tickwise never interprets them. */
	UFUNCTION(BlueprintCallable, Category = "Tickwise")
	void SetInputs(const TArray<uint8>& Inputs);

	/**
	 * Records one tick with the current inputs and the probe's hashes.
	 * Call it once per simulation tick, in order, after the step has run.
	 */
	UFUNCTION(BlueprintCallable, Category = "Tickwise")
	bool RecordTick();

	/** Records a named point in the recording, for example a round start. */
	UFUNCTION(BlueprintCallable, Category = "Tickwise")
	void RecordMarker(const FString& Label);

	UFUNCTION(BlueprintPure, Category = "Tickwise")
	bool IsRecording() const;

	/** Ticks recorded so far, which is also the tick the next RecordTick uses. */
	UFUNCTION(BlueprintPure, Category = "Tickwise")
	int64 GetTick() const;

	/** The last failure, or an empty string. Recording stops at the first failure and the game keeps running. */
	UFUNCTION(BlueprintPure, Category = "Tickwise")
	FString GetLastError() const;

	/** The resolved absolute path of the current or last recording. */
	UFUNCTION(BlueprintPure, Category = "Tickwise")
	FString GetRecordingPath() const;

	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;
	virtual void EndPlay(const EEndPlayReason::Type EndPlayReason) override;

private:
	UObject* ResolveProbe() const;
	void Fail(const FString& Message);

	tickwise::Recorder Recorder;
	TArray<uint8> PendingInputs;
	uint64 NextTick = 0;
	FString LastError;
	FString RecordingPath;
};
