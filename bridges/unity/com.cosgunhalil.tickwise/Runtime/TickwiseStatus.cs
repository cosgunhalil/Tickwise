using System;

namespace Tickwise
{
    /// <summary>
    /// Result of a native call. Zero is success; anything else means the
    /// call did nothing and the wrapper raised a <see cref="TickwiseException"/>.
    /// </summary>
    public enum TickwiseStatus
    {
        /// <summary>The call succeeded.</summary>
        Ok = 0,
        /// <summary>A required pointer was null. A wrapper bug if it ever surfaces.</summary>
        NullPointer = 1,
        /// <summary>An argument was out of range, for example a marker label over 65535 bytes or an empty dump path.</summary>
        InvalidArgument = 2,
        /// <summary>A string was not valid UTF-8. Cannot happen from C# strings.</summary>
        InvalidUtf8 = 3,
        /// <summary>Creating, reading, writing, or finishing a file failed, or a recording is malformed.</summary>
        Io = 4,
        /// <summary>Ticks must advance by exactly one between record calls.</summary>
        NonSequentialTick = 5,
        /// <summary>The recorder or replayer was already finished.</summary>
        AlreadyFinished = 6,
        /// <summary>The native library panicked internally. Please report it with the message.</summary>
        Panic = 7,
        /// <summary>A replay produced a hash the recording does not have at that tick: the simulation is not reproducing the session.</summary>
        HashMismatch = 8,
        /// <summary>The recording's input format id differs from the one the replay expects.</summary>
        InputFormatMismatch = 9,
        /// <summary>A tick lies outside the recording, for a dump request or a seek.</summary>
        TickOutOfRange = 10,
        /// <summary>The replay protocol was broken: AfterTick without a step, or a step left without its AfterTick at finish.</summary>
        ProtocolMisuse = 11,
        /// <summary>The replay owes a dump at this tick and none was given. The step stays pending, so the call can be repeated with one.</summary>
        MissingDump = 12,
        /// <summary>The recording holds no ticks.</summary>
        EmptyRecording = 13,
    }

    /// <summary>Raised when the native library reports a failure.</summary>
    public sealed class TickwiseException : Exception
    {
        /// <summary>The status the native call returned.</summary>
        public TickwiseStatus Status { get; }

        public TickwiseException(TickwiseStatus status, string message)
            : base(message)
        {
            Status = status;
        }

        /// <summary>
        /// Builds the exception for a failed call from the status and the
        /// native library's last error message on this thread.
        /// </summary>
        internal static TickwiseException FromLastError(TickwiseStatus status, string operation)
        {
            string name = Native.ReadUtf8(Native.tickwise_status_name(status));
            string detail = Native.LastErrorMessage();
            // The native message already names the failure, so the status
            // name is only added when there is no message to show.
            string message = detail.Length == 0
                ? $"{operation} failed with {name}"
                : $"{operation} failed: {detail}";
            return new TickwiseException(status, message);
        }

        /// <summary>Throws when the status is anything but Ok.</summary>
        internal static void ThrowIfFailed(TickwiseStatus status, string operation)
        {
            if (status != TickwiseStatus.Ok)
            {
                throw FromLastError(status, operation);
            }
        }
    }
}
