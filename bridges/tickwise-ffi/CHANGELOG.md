# Changelog

All notable changes to `tickwise-ffi` are documented in this file. The C surface is versioned separately from the Rust crates; see the ABI versioning section of the README.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Crate skeleton built as cdylib, staticlib, and rlib, with `unsafe_op_in_unsafe_fn` and `undocumented_unsafe_blocks` denied.
- `tickwise_ffi_abi_version` and `tickwise_ffi_version` queries. ABI version 1.
- The Pass 1 recorder surface: `tickwise_recorder_config_default`, `tickwise_recorder_create`, `tickwise_recorder_record_tick` with caller-provided hashes, `tickwise_recorder_wants_full_hash`, `tickwise_recorder_wants_snapshot`, `tickwise_recorder_record_snapshot`, `tickwise_recorder_record_marker`, `tickwise_recorder_finish`, `tickwise_recorder_destroy`.
- `TickwiseStatus` codes, `tickwise_last_error_message` per thread, and `tickwise_status_name`. Every call catches panics and reports them as `Panic` instead of unwinding across the boundary.
- `tickwise_xxh3_64` and the `TICKWISE_HASH_ALGO_*` constants, so engine users hash under `hash_algo_id` 1 exactly like the Rust serde layer.
