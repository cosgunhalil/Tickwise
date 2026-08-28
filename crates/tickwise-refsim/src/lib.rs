//! Reference simulation for Tickwise.
//!
//! A deliberately simple deterministic 2D physics world with a fixed tick
//! and a self-contained LCG random generator. It is the integration-test
//! bed for the whole kit and the source of every documentation example.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
