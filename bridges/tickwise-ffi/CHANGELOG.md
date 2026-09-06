# Changelog

All notable changes to `tickwise-ffi` are documented in this file. The C surface is versioned separately from the Rust crates; see the ABI versioning section of the README.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Crate skeleton built as cdylib, staticlib, and rlib, with `unsafe_op_in_unsafe_fn` and `undocumented_unsafe_blocks` denied.
- `tickwise_ffi_abi_version` and `tickwise_ffi_version` queries. ABI version 1.
