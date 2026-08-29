//! Reference simulation for Tickwise.
//!
//! A deliberately simple deterministic 2D physics world with a fixed tick
//! and a self-contained LCG random generator. It is the integration-test
//! bed for the whole kit and the source of every documentation example.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod lcg;
pub mod world;

pub use lcg::Lcg;
pub use world::{Ball, DT, Player, PlayerInput, TICKS_PER_SECOND, Vec2, World, WorldConfig};
