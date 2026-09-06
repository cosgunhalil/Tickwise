//! Hashing helpers so engine users hash with the same algorithm as Rust
//! users, under the same `hash_algo_id`.

/// `hash_algo_id` for a caller-defined hash. Nothing is assumed about it.
pub const TICKWISE_HASH_ALGO_USER_DEFINED: u16 = 0;

/// `hash_algo_id` for xxh3 64 bit, what [`tickwise_xxh3_64`] computes and
/// what the core's serde layer uses by default.
pub const TICKWISE_HASH_ALGO_XXH3: u16 = 1;

/// `hash_algo_id` for blake3 truncated to 64 bits. Not exposed here yet.
pub const TICKWISE_HASH_ALGO_BLAKE3: u16 = 2;

/// Computes the xxh3 64 bit hash of `len` bytes at `data`, identical to
/// the hash the Rust serde layer produces for the same bytes.
///
/// A null `data` with `len == 0` hashes the empty input. A null `data`
/// with a nonzero `len` returns zero, because there is no status channel
/// on this call; do not pass that.
///
/// # Safety
///
/// `data` must be valid for `len` readable bytes, or null with `len == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_xxh3_64(data: *const u8, len: usize) -> u64 {
    if len == 0 {
        return xxhash_rust::xxh3::xxh3_64(&[]);
    }
    if data.is_null() {
        return 0;
    }
    // SAFETY: data is non-null and the caller promises it is valid for
    // len readable bytes during this call.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    xxhash_rust::xxh3::xxh3_64(bytes)
}
