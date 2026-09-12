namespace Tickwise
{
    /// <summary>Information about the loaded native library.</summary>
    public static class TickwiseNative
    {
        /// <summary>
        /// The C surface version this package was written against. The native
        /// library must report the same value or every call is refused.
        /// </summary>
        public const uint ExpectedAbiVersion = 2;

        /// <summary>The C surface version the loaded native library reports.</summary>
        public static uint AbiVersion => Native.tickwise_ffi_abi_version();

        /// <summary>The native library's own version, for example 0.1.0.</summary>
        public static string Version => Native.ReadUtf8(Native.tickwise_ffi_version());

        /// <summary>
        /// Throws when the loaded native library speaks a different C surface
        /// version than this package expects. Called once on the first
        /// recorder creation; call it yourself at startup for an earlier,
        /// clearer failure.
        /// </summary>
        public static void EnsureCompatible()
        {
            uint actual = AbiVersion;
            if (actual != ExpectedAbiVersion)
            {
                throw new TickwiseException(
                    TickwiseStatus.InvalidArgument,
                    $"tickwise_ffi reports ABI version {actual}, this package expects {ExpectedAbiVersion}. " +
                    "Update the package and the native library together.");
            }
        }
    }
}
