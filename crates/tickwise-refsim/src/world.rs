//! The deterministic 2D physics world.
//!
//! Deliberately simple: balls bounce inside a rectangular arena, players
//! move by input and deflect balls on contact. Physical realism is a non
//! goal. Bit-exact determinism is the whole point, so the math uses only
//! addition, subtraction, multiplication, and comparisons. No square
//! roots, no trigonometry, nothing that could vary across platforms.

use crate::chaos::{ChaosConfig, ChaosMode};
use crate::lcg::Lcg;
use tickwise::{DeterminismProbe, StateDump, Value};

/// Fixed simulation rate in ticks per second.
pub const TICKS_PER_SECOND: u32 = 60;

/// Fixed timestep in seconds, derived from [`TICKS_PER_SECOND`].
pub const DT: f32 = 1.0 / TICKS_PER_SECOND as f32;

/// A 2D vector of two floats.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    /// Horizontal component.
    pub x: f32,
    /// Vertical component.
    pub y: f32,
}

/// A bouncing ball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ball {
    /// Center position in world units.
    pub position: Vec2,
    /// Velocity in world units per second.
    pub velocity: Vec2,
}

/// A player-controlled circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    /// Center position in world units.
    pub position: Vec2,
}

/// One player's input for one tick.
///
/// Components are clamped to the range -1 to 1 when applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerInput {
    /// Horizontal movement direction.
    pub move_x: i8,
    /// Vertical movement direction.
    pub move_y: i8,
}

/// Configuration for a new world.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldConfig {
    /// Arena width in world units.
    pub width: f32,
    /// Arena height in world units.
    pub height: f32,
    /// Number of balls to spawn.
    pub ball_count: u32,
    /// Number of players to spawn.
    pub player_count: u32,
    /// Seed for the world's random generator.
    pub seed: u64,
    /// Optional non-determinism injection, see [`ChaosConfig`].
    pub chaos: Option<ChaosConfig>,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            width: 100.0,
            height: 60.0,
            ball_count: 8,
            player_count: 2,
            seed: 0,
            chaos: None,
        }
    }
}

const BALL_RADIUS: f32 = 0.5;
const PLAYER_RADIUS: f32 = 1.0;
const PLAYER_SPEED: f32 = 8.0;
const MIN_BALL_SPEED: f32 = -6.0;
const MAX_BALL_SPEED: f32 = 6.0;
const WALL_BOUNCE_SCORE: u64 = 1;
const DEFLECT_SCORE: u64 = 10;

/// The deterministic simulation state.
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    config: WorldConfig,
    balls: Vec<Ball>,
    players: Vec<Player>,
    rng: Lcg,
    tick: u64,
    score: u64,
    scratch: u64,
}

impl World {
    /// Creates a world and spawns its entities from the configured seed.
    pub fn new(config: WorldConfig) -> Self {
        let mut rng = Lcg::new(config.seed);

        let balls = (0..config.ball_count)
            .map(|_| Ball {
                position: Vec2 {
                    x: rng.next_f32_in(BALL_RADIUS, config.width - BALL_RADIUS),
                    y: rng.next_f32_in(BALL_RADIUS, config.height - BALL_RADIUS),
                },
                velocity: Vec2 {
                    x: rng.next_f32_in(MIN_BALL_SPEED, MAX_BALL_SPEED),
                    y: rng.next_f32_in(MIN_BALL_SPEED, MAX_BALL_SPEED),
                },
            })
            .collect();

        let players = (0..config.player_count)
            .map(|_| Player {
                position: Vec2 {
                    x: rng.next_f32_in(PLAYER_RADIUS, config.width - PLAYER_RADIUS),
                    y: rng.next_f32_in(PLAYER_RADIUS, config.height - PLAYER_RADIUS),
                },
            })
            .collect();

        Self {
            config,
            balls,
            players,
            rng,
            tick: 0,
            score: 0,
            scratch: 0,
        }
    }

    fn active_chaos(&self) -> Option<ChaosMode> {
        match &self.config.chaos {
            Some(chaos) if self.tick >= chaos.start_tick => Some(chaos.mode),
            _ => None,
        }
    }

    /// Advances the simulation by one tick.
    ///
    /// Inputs map to players by index. Missing inputs count as neutral and
    /// extra inputs are ignored, so replaying a recording with a different
    /// player count stays well defined.
    pub fn step(&mut self, inputs: &[PlayerInput]) {
        let chaos = self.active_chaos();

        // Chaos: uninit-read. The scratch value is leftover from the
        // previous tick, and the canonical path never reads it. Reading
        // it here is the stale-value bug: state that should have been
        // initialized this tick, but was not. The or with 1 guarantees a
        // nonzero contribution, so the strike lands on its start tick.
        if chaos == Some(ChaosMode::UninitRead) {
            self.score = self.score.wrapping_add(self.scratch | 1);
        }

        for (index, player) in self.players.iter_mut().enumerate() {
            let input = inputs.get(index).copied().unwrap_or_default();
            let dx = input.move_x.clamp(-1, 1) as f32;
            let dy = input.move_y.clamp(-1, 1) as f32;
            player.position.x = (player.position.x + dx * PLAYER_SPEED * DT)
                .clamp(PLAYER_RADIUS, self.config.width - PLAYER_RADIUS);
            player.position.y = (player.position.y + dy * PLAYER_SPEED * DT)
                .clamp(PLAYER_RADIUS, self.config.height - PLAYER_RADIUS);
        }

        for ball in &mut self.balls {
            ball.position.x += ball.velocity.x * DT;
            ball.position.y += ball.velocity.y * DT;

            if ball.position.x < BALL_RADIUS {
                ball.position.x = BALL_RADIUS;
                ball.velocity.x = -ball.velocity.x;
                self.score += WALL_BOUNCE_SCORE;
            } else if ball.position.x > self.config.width - BALL_RADIUS {
                ball.position.x = self.config.width - BALL_RADIUS;
                ball.velocity.x = -ball.velocity.x;
                self.score += WALL_BOUNCE_SCORE;
            }

            if ball.position.y < BALL_RADIUS {
                ball.position.y = BALL_RADIUS;
                ball.velocity.y = -ball.velocity.y;
                self.score += WALL_BOUNCE_SCORE;
            } else if ball.position.y > self.config.height - BALL_RADIUS {
                ball.position.y = self.config.height - BALL_RADIUS;
                ball.velocity.y = -ball.velocity.y;
                self.score += WALL_BOUNCE_SCORE;
            }

            for player in &self.players {
                let dx = ball.position.x - player.position.x;
                let dy = ball.position.y - player.position.y;
                let contact = BALL_RADIUS + PLAYER_RADIUS;
                if dx * dx + dy * dy < contact * contact {
                    ball.velocity.x = -ball.velocity.x;
                    ball.velocity.y = -ball.velocity.y;
                    ball.position.x += ball.velocity.x * DT;
                    ball.position.y += ball.velocity.y * DT;
                    self.score += DEFLECT_SCORE;
                }
            }
        }

        match chaos {
            // Chaos: float-drift. One ULP of velocity per tick, the
            // shape of cross-platform float deviation. Too small for the
            // light hash to notice, which is exactly the point.
            Some(ChaosMode::FloatDrift) => {
                if let Some(ball) = self.balls.first_mut() {
                    ball.velocity.x *= 1.0 + f32::EPSILON;
                }
            }
            // Chaos: hashmap-iter. Ball contributions folded through a
            // real HashMap in iteration order, which is random per
            // process, into an order-sensitive accumulator. This block
            // deliberately violates the project determinism rules.
            Some(ChaosMode::HashmapIter) => {
                let mut map = std::collections::HashMap::new();
                for (index, ball) in self.balls.iter().enumerate() {
                    map.insert(index as u64, u64::from(ball.position.x.to_bits()));
                }
                let mut acc = self.score;
                for (key, value) in &map {
                    acc = acc.rotate_left(7) ^ key.wrapping_mul(31) ^ value;
                }
                self.score = acc;
            }
            // Chaos: time-dependent. The wall clock reseeds the RNG,
            // the classic leak of real time into simulated time.
            Some(ChaosMode::TimeDependent) => {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0x5EED)
                    | 1;
                self.rng = Lcg::new(self.rng.state() ^ nanos);
            }
            Some(ChaosMode::UninitRead) | None => {}
        }

        // The scratch value every tick leaves behind for the next one.
        // Canonical code never reads it, only the uninit-read chaos does.
        self.scratch = match self.balls.first() {
            Some(ball) => {
                (u64::from(ball.position.x.to_bits()) << 32) | u64::from(ball.velocity.y.to_bits())
            }
            None => self.tick | 1,
        };

        self.tick += 1;
    }

    /// Returns the number of ticks simulated so far.
    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    /// Returns the current score.
    pub fn score(&self) -> u64 {
        self.score
    }

    /// Returns the balls, for tests and future dump support.
    pub fn balls(&self) -> &[Ball] {
        &self.balls
    }

    /// Returns the players, for tests and future dump support.
    pub fn players(&self) -> &[Player] {
        &self.players
    }
}

/// A tiny FNV-1a digest, the refsim's self-contained hash.
struct Digest(u64);

impl Digest {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn write_f32(&mut self, value: f32) {
        self.write_u64(u64::from(value.to_bits()));
    }

    fn finish(self) -> u64 {
        self.0
    }
}

impl DeterminismProbe for World {
    fn light_hash(&self) -> u64 {
        let mut digest = Digest::new();
        digest.write_u64(self.tick);
        digest.write_u64(self.score);
        digest.write_u64(self.balls.len() as u64);
        digest.write_u64(self.players.len() as u64);
        digest.write_u64(self.rng.state());
        for player in &self.players {
            digest.write_f32(player.position.x);
            digest.write_f32(player.position.y);
        }
        digest.finish()
    }

    fn full_hash(&self) -> u64 {
        let mut digest = Digest::new();
        digest.write_u64(self.tick);
        digest.write_u64(self.score);
        digest.write_u64(self.rng.state());
        for ball in &self.balls {
            digest.write_f32(ball.position.x);
            digest.write_f32(ball.position.y);
            digest.write_f32(ball.velocity.x);
            digest.write_f32(ball.velocity.y);
        }
        for player in &self.players {
            digest.write_f32(player.position.x);
            digest.write_f32(player.position.y);
        }
        digest.finish()
    }

    fn state_dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        dump.insert("tick", self.tick);
        dump.insert("score", self.score);
        dump.insert("rng.state", self.rng.state());

        dump.insert("balls", Value::Len(self.balls.len() as u64));
        for (i, ball) in self.balls.iter().enumerate() {
            dump.insert(format!("balls[{i}].position.x"), ball.position.x);
            dump.insert(format!("balls[{i}].position.y"), ball.position.y);
            dump.insert(format!("balls[{i}].velocity.x"), ball.velocity.x);
            dump.insert(format!("balls[{i}].velocity.y"), ball.velocity.y);
        }

        dump.insert("players", Value::Len(self.players.len() as u64));
        for (i, player) in self.players.iter().enumerate() {
            dump.insert(format!("players[{i}].position.x"), player.position.x);
            dump.insert(format!("players[{i}].position.y"), player.position.y);
        }
        dump
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_spawn_inside_the_arena() {
        let world = World::new(WorldConfig::default());
        let config = WorldConfig::default();
        for ball in world.balls() {
            assert!(ball.position.x >= BALL_RADIUS);
            assert!(ball.position.x <= config.width - BALL_RADIUS);
            assert!(ball.position.y >= BALL_RADIUS);
            assert!(ball.position.y <= config.height - BALL_RADIUS);
        }
    }

    #[test]
    fn players_move_with_input() {
        let config = WorldConfig::default();
        let mut world = World::new(config.clone());
        let before = world.players()[0].position;
        let inputs = [PlayerInput {
            move_x: 1,
            move_y: 0,
        }];
        world.step(&inputs);
        let after = world.players()[0].position;
        let expected_x =
            (before.x + PLAYER_SPEED * DT).clamp(PLAYER_RADIUS, config.width - PLAYER_RADIUS);
        assert_eq!(after.x, expected_x);
        assert_eq!(after.y, before.y);
        assert_eq!(world.tick_count(), 1);
    }

    #[test]
    fn balls_stay_inside_the_arena() {
        let config = WorldConfig::default();
        let mut world = World::new(config.clone());
        for _ in 0..5000 {
            world.step(&[]);
        }
        for ball in world.balls() {
            assert!(ball.position.x >= BALL_RADIUS - f32::EPSILON);
            assert!(ball.position.x <= config.width - BALL_RADIUS + f32::EPSILON);
            assert!(ball.position.y >= BALL_RADIUS - f32::EPSILON);
            assert!(ball.position.y <= config.height - BALL_RADIUS + f32::EPSILON);
        }
    }

    #[test]
    fn state_dump_covers_every_entity_and_matches_between_twins() {
        let config = WorldConfig::default();
        let mut a = World::new(config.clone());
        let mut b = World::new(config.clone());
        for _ in 0..100 {
            a.step(&[]);
            b.step(&[]);
        }

        let dump = a.state_dump();
        // 3 scalars, 1 length + 4 fields per ball, 1 length + 2 per player.
        let expected =
            3 + 1 + 4 * config.ball_count as usize + 1 + 2 * config.player_count as usize;
        assert_eq!(dump.len(), expected);
        assert_eq!(dump.get("tick"), Some(&Value::U64(100)));
        assert_eq!(
            dump.get("balls"),
            Some(&Value::Len(u64::from(config.ball_count)))
        );
        assert_eq!(dump, b.state_dump());

        b.step(&[]);
        assert_ne!(dump, b.state_dump());
    }

    #[test]
    fn hashes_change_when_state_changes() {
        let mut world = World::new(WorldConfig::default());
        let before_light = world.light_hash();
        let before_full = world.full_hash();
        world.step(&[]);
        assert_ne!(world.light_hash(), before_light);
        assert_ne!(world.full_hash(), before_full);
    }
}
