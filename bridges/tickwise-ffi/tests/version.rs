//! The version queries are the first thing every bridge calls, so they
//! are tested from Rust before any C harness exists.

use std::ffi::CStr;
use tickwise_ffi::{ABI_VERSION, tickwise_ffi_abi_version, tickwise_ffi_version};

#[test]
fn abi_version_matches_the_constant() {
    assert_eq!(tickwise_ffi_abi_version(), ABI_VERSION);
    assert_eq!(ABI_VERSION, 1);
}

#[test]
fn version_string_is_the_crate_version() {
    let ptr = tickwise_ffi_version();
    assert!(!ptr.is_null());
    // SAFETY: the function documents that the pointer refers to a static
    // NUL-terminated string that lives for the whole process.
    let version = unsafe { CStr::from_ptr(ptr) };
    assert_eq!(version.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
}
