//! Tickwise driving the GGRS `ex_game` example through a real
//! `SyncTestSession`.
//!
//! The game logic is ported from `examples/ex_game/ex_game.rs` in the
//! GGRS repository, copyright the GGRS authors, dual licensed under MIT
//! OR Apache-2.0. Only the simulation is ported; the macroquad rendering
//! is left behind so the integration runs headless in CI.
//!
//! What this proves: Tickwise records a rollback session correctly. GGRS
//! rolls back and re-simulates frames every step, so the harness records
//! a tick only the first time a frame is reached, and on every
//! re-simulation compares the live hash against the recorded one. The
//! checksum GGRS itself verifies is the Tickwise full hash.

use ggrs::{
    Config, Frame, GameStateCell, GgrsRequest, InputStatus, PlayerHandle, PredictRepeatLast,
    SessionBuilder,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::net::SocketAddr;
use tickwise::format::RecReader;
use tickwise::serde_probe::{HashAlgo, SerdeProbe, format_id};
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, ReplayConfig, Replayer, StateDump};

const FPS: u64 = 60;
const WINDOW_HEIGHT: f32 = 800.0;
const WINDOW_WIDTH: f32 = 600.0;
const INPUT_UP: u8 = 1 << 0;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_LEFT: u8 = 1 << 2;
const INPUT_RIGHT: u8 = 1 << 3;
const MOVEMENT_SPEED: f32 = 15.0 / FPS as f32;
const ROTATION_SPEED: f32 = 2.5 / FPS as f32;
const MAX_SPEED: f32 = 7.0;
const FRICTION: f32 = 0.98;

/// The input format label. Bump it if the encoding below changes.
pub const INPUT_FORMAT_LABEL: &str = "ggrs ex_game Input v1";

/// One byte of directional bit flags, exactly as in ex_game.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Debug)]
pub struct Input {
    /// Bit flags: up, down, left, right in bits 0 to 3.
    pub inp: u8,
}

/// The ex_game state, ported verbatim in shape.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct State {
    /// Frame counter, GGRS style.
    pub frame: i32,
    /// Number of players.
    pub num_players: usize,
    /// Per-player positions.
    pub positions: Vec<(f32, f32)>,
    /// Per-player velocities.
    pub velocities: Vec<(f32, f32)>,
    /// Per-player rotations in radians.
    pub rotations: Vec<f32>,
}

impl State {
    /// Spawns players spread across the arena, facing right.
    pub fn new(num_players: usize) -> Self {
        let positions = (0..num_players)
            .map(|i| {
                let x = WINDOW_WIDTH * (i as f32 + 1.0) / (num_players as f32 + 1.0);
                (x, WINDOW_HEIGHT * 0.5)
            })
            .collect();
        Self {
            frame: 0,
            num_players,
            positions,
            velocities: vec![(0.0, 0.0); num_players],
            rotations: vec![0.0; num_players],
        }
    }

    /// The ex_game physics step, ported from the GGRS example. Uses sin,
    /// cos, and sqrt on purpose: the upstream example notes that this
    /// float math may desync across architectures, which is exactly the
    /// kind of thing Tickwise exists to catch.
    pub fn advance(&mut self, inputs: &[(Input, InputStatus)]) {
        self.frame += 1;
        for (i, player_input) in inputs.iter().enumerate().take(self.num_players) {
            let input = match player_input.1 {
                InputStatus::Confirmed | InputStatus::Predicted => player_input.0.inp,
                InputStatus::Disconnected => 4,
            };
            let (old_x, old_y) = self.positions[i];
            let (old_vel_x, old_vel_y) = self.velocities[i];
            let mut rot = self.rotations[i];
            let mut vel_x = old_vel_x * FRICTION;
            let mut vel_y = old_vel_y * FRICTION;
            if input & INPUT_UP != 0 && input & INPUT_DOWN == 0 {
                vel_x += MOVEMENT_SPEED * rot.cos();
                vel_y += MOVEMENT_SPEED * rot.sin();
            }
            if input & INPUT_UP == 0 && input & INPUT_DOWN != 0 {
                vel_x -= MOVEMENT_SPEED * rot.cos();
                vel_y -= MOVEMENT_SPEED * rot.sin();
            }
            if input & INPUT_LEFT != 0 && input & INPUT_RIGHT == 0 {
                rot = (rot - ROTATION_SPEED).rem_euclid(2.0 * std::f32::consts::PI);
            }
            if input & INPUT_LEFT == 0 && input & INPUT_RIGHT != 0 {
                rot = (rot + ROTATION_SPEED).rem_euclid(2.0 * std::f32::consts::PI);
            }
            let magnitude = (vel_x * vel_x + vel_y * vel_y).sqrt();
            if magnitude > MAX_SPEED {
                vel_x = (vel_x * MAX_SPEED) / magnitude;
                vel_y = (vel_y * MAX_SPEED) / magnitude;
            }
            let x = (old_x + vel_x).clamp(0.0, WINDOW_WIDTH);
            let y = (old_y + vel_y).clamp(0.0, WINDOW_HEIGHT);
            self.positions[i] = (x, y);
            self.velocities[i] = (vel_x, vel_y);
            self.rotations[i] = rot;
        }
    }
}

/// GGRS configuration for the ported game.
pub struct ExGameConfig;

impl Config for ExGameConfig {
    type Input = Input;
    type InputPredictor = PredictRepeatLast;
    type State = State;
    type Address = SocketAddr;
}

/// Deterministic scripted inputs standing in for keyboards.
pub fn scripted_input(frame: Frame, player: PlayerHandle) -> Input {
    let phase = (frame / 13) as usize + player * 3;
    Input {
        inp: (phase % 16) as u8,
    }
}

/// An optional deterministic defect: from the given frame on, player
/// zero's x position gains half a unit per frame. Applied on every
/// advance, including re-simulations, so GGRS itself never notices.
fn apply_bug(state: &mut State, bug_at: Option<Frame>) {
    if let Some(at) = bug_at
        && state.frame >= at
    {
        state.positions[0].0 = (state.positions[0].0 + 0.5).min(WINDOW_WIDTH);
    }
}

fn recorder_config() -> RecorderConfig {
    RecorderConfig {
        full_hash_interval: 30,
        hash_algo_id: HashAlgo::Xxh3.id(),
        input_format_id: format_id(INPUT_FORMAT_LABEL),
        ..RecorderConfig::default()
    }
}

/// What a session run produced.
pub struct SessionOutcome {
    /// The `.rec` bytes Tickwise recorded.
    pub recording: Vec<u8>,
    /// Frames recorded for the first time.
    pub frames_recorded: u64,
    /// Frames GGRS rolled back and re-simulated, each verified against
    /// the recorded hash.
    pub resimulated_frames: u64,
    /// The state at the end of the session.
    pub final_state: State,
}

/// Runs a GGRS synctest session for the given number of frames while
/// Tickwise records it.
pub fn run_synctest_session(
    num_players: usize,
    frames: usize,
    check_distance: usize,
    bug_at: Option<Frame>,
) -> Result<SessionOutcome, Box<dyn std::error::Error>> {
    let mut session = SessionBuilder::<ExGameConfig>::new()
        .with_num_players(num_players)?
        .with_check_distance(check_distance)
        .with_input_delay(2)
        .start_synctest_session()?;

    let mut state = State::new(num_players);
    let mut rec = Recorder::new(Vec::new(), recorder_config())?;
    let mut recorded_hashes: BTreeMap<u64, u64> = BTreeMap::new();
    let mut highest_frame: Frame = 0;
    let mut resimulated_frames: u64 = 0;

    for _ in 0..frames {
        for handle in 0..num_players {
            session.add_local_input(handle, scripted_input(session.current_frame(), handle))?;
        }
        for request in session.advance_frame()? {
            match request {
                GgrsRequest::SaveGameState { cell, frame } => {
                    save_state(&state, cell, frame);
                }
                GgrsRequest::LoadGameState { cell, .. } => {
                    state = cell.load().ok_or("GGRS asked to load an empty cell")?;
                }
                GgrsRequest::AdvanceFrame { inputs } => {
                    state.advance(&inputs);
                    apply_bug(&mut state, bug_at);
                    let tick = state.frame as u64;
                    let probe = SerdeProbe::new(&state);
                    if state.frame > highest_frame {
                        highest_frame = state.frame;
                        let bytes: Vec<u8> = inputs.iter().map(|(input, _)| input.inp).collect();
                        rec.record_tick(tick, &bytes, &probe)?;
                        recorded_hashes.insert(tick, probe.light_hash());
                    } else {
                        resimulated_frames += 1;
                        let recorded = recorded_hashes
                            .get(&tick)
                            .ok_or("re-simulated a frame that was never recorded")?;
                        if probe.light_hash() != *recorded {
                            return Err(format!(
                                "re-simulation of frame {tick} diverged from the recording"
                            )
                            .into());
                        }
                    }
                }
            }
        }
    }

    Ok(SessionOutcome {
        recording: rec.finish()?,
        frames_recorded: highest_frame as u64,
        resimulated_frames,
        final_state: state,
    })
}

fn save_state(state: &State, cell: GameStateCell<State>, frame: Frame) {
    assert_eq!(
        state.frame, frame,
        "GGRS and the game disagree on the frame"
    );
    let checksum = SerdeProbe::new(state).full_hash();
    cell.save(frame, Some(state.clone()), Some(u128::from(checksum)));
}

/// Replays a recording through a plain game loop with no GGRS involved,
/// verifying every hash, and returns the dump at the requested tick.
pub fn replay_recording(
    recording: &[u8],
    num_players: usize,
    dump_at: u64,
    bug_at: Option<Frame>,
) -> Result<StateDump, Box<dyn std::error::Error>> {
    let mut reader = RecReader::open(Cursor::new(recording))?;
    let mut rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            dump_at_ticks: vec![dump_at],
            verify_hashes: true,
            expected_input_format_id: Some(format_id(INPUT_FORMAT_LABEL)),
        },
    )?;
    let mut state = State::new(num_players);
    while let Some(step) = rep.next_step() {
        let inputs: Vec<(Input, InputStatus)> = step
            .inputs()
            .iter()
            .map(|inp| (Input { inp: *inp }, InputStatus::Confirmed))
            .collect();
        state.advance(&inputs);
        apply_bug(&mut state, bug_at);
        rep.after_tick(&SerdeProbe::new(&state))?;
    }
    let mut dumps = rep.into_dumps()?;
    Ok(dumps.remove(0).1)
}
