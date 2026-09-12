//! The recorder node, the whole surface a game touches from GDScript or
//! C#.

// gdext generates a getter and a setter for every exported field, and
// those generated methods carry no doc comments of their own, so the
// crate-wide missing_docs rule cannot apply here. Everything written by
// hand in this module is documented all the same.
#![allow(missing_docs)]

use crate::probe::GodotProbe;
use godot::builtin::{GString, PackedByteArray, StringName};
use godot::classes::{INode, Node, ProjectSettings};
use godot::prelude::*;
use std::io::BufWriter;
use tickwise::recorder::{Recorder, RecorderConfig};
use tickwise::{DeterminismProbe, SessionMeta};

/// Records one `.rec` file per session, one tick per physics frame.
///
/// Add the node to your scene, put your simulation nodes in the
/// `tickwise` group, and call `start_recording`. The node records at the
/// end of every physics frame, so it sees the state your simulation just
/// produced.
///
/// ```gdscript
/// extends Node
///
/// @onready var tickwise: TickwiseRecorder = $TickwiseRecorder
///
/// func _ready() -> void:
///     tickwise.game_id = "my-game"
///     tickwise.rng_seed = 12345
///     tickwise.start_recording("user://clean.rec")
///
/// func _physics_process(_delta: float) -> void:
///     tickwise.set_inputs(PackedByteArray([input_bits]))
/// ```
///
/// Two recordings of the same match then go to the command line tool:
///
/// ```text
/// tickwise compare clean.rec chaotic.rec
/// ```
#[derive(GodotClass)]
#[class(base=Node)]
pub struct TickwiseRecorder {
    /// Identifier of the game, written into the recording header.
    #[export]
    game_id: GString,

    /// Build identifier, for example a git hash or a version string.
    /// Comparing recordings from different builds is a warning, not an
    /// error, and this is what makes that visible.
    #[export]
    build_hash: GString,

    /// Simulation rate in ticks per second. Metadata only. Leave it at
    /// zero to take the project's physics tick rate at recording time.
    #[export]
    tick_rate: i64,

    /// The seed the simulation started from. Two recordings with
    /// different seeds were never going to agree, and `compare` says so.
    #[export]
    rng_seed: i64,

    /// How often a full hash is recorded. Zero disables full hashes,
    /// which leaves the light hash blind spot uncovered.
    #[export]
    full_hash_interval: i64,

    /// Your identifier for the input encoding. Change it whenever the
    /// bytes change meaning, so a later replay refuses a recording made
    /// with the old encoding instead of misreading it.
    #[export]
    input_format_id: i64,

    /// How often a full state dump is recorded. Zero records none. With
    /// dumps in both recordings, `tickwise diff a.rec b.rec` reaches field
    /// level with no replay. Each dump walks every covered node, so keep
    /// the interval generous and measure it.
    #[export]
    dump_interval: i64,

    /// The group whose nodes are covered by the full hash and the dump.
    #[export]
    group: StringName,

    /// The group whose nodes are also covered by the light hash, which
    /// runs every tick. Keep it small.
    #[export]
    light_group: StringName,

    recorder: Option<Recorder<BufWriter<std::fs::File>>>,
    inputs: Vec<u8>,
    next_tick: u64,
    last_error: String,
    base: Base<Node>,
}

#[godot_api]
impl INode for TickwiseRecorder {
    fn init(base: Base<Node>) -> Self {
        Self {
            game_id: GString::new(),
            build_hash: GString::new(),
            tick_rate: 0,
            rng_seed: 0,
            full_hash_interval: 300,
            input_format_id: 0,
            dump_interval: 0,
            group: StringName::from("tickwise"),
            light_group: StringName::from("tickwise_light"),
            recorder: None,
            inputs: Vec::new(),
            next_tick: 0,
            last_error: String::new(),
            base,
        }
    }

    fn ready(&mut self) {
        // Run after every other physics callback, so the recording sees
        // the state the simulation just finished producing.
        self.base_mut().set_physics_process_priority(1_000_000);
    }

    fn physics_process(&mut self, _delta: f64) {
        if self.recorder.is_none() {
            return;
        }
        let Some(tree) = self.base().get_tree_or_null() else {
            return;
        };
        let probe = GodotProbe::collect(&tree, &self.group.clone(), &self.light_group.clone());
        let tick = self.next_tick;
        let inputs = std::mem::take(&mut self.inputs);

        let result = self
            .recorder
            .as_mut()
            .expect("checked above")
            .record_tick(tick, &inputs, &probe);
        self.inputs = inputs;

        match result {
            Ok(()) => self.next_tick += 1,
            Err(err) => self.fail(&err.to_string()),
        }
    }

    fn exit_tree(&mut self) {
        // Leaving the tree ends the session, so a game that just quits
        // still leaves a readable recording.
        self.stop_recording();
    }
}

#[godot_api]
impl TickwiseRecorder {
    /// Opens a recording at `path` and starts recording on the next
    /// physics frame. Returns false on failure; `get_last_error` says
    /// why.
    ///
    /// A `res://` or `user://` path is resolved the way Godot resolves
    /// it, so `user://session.rec` lands in the user data folder.
    #[func]
    pub fn start_recording(&mut self, path: GString) -> bool {
        self.stop_recording();
        self.last_error.clear();

        let resolved = ProjectSettings::singleton()
            .globalize_path(&path)
            .to_string();
        if resolved.is_empty() {
            self.fail("the recording path is empty");
            return false;
        }

        let tick_rate = if self.tick_rate > 0 {
            self.tick_rate as u32
        } else {
            godot::classes::Engine::singleton().get_physics_ticks_per_second() as u32
        };

        let config = RecorderConfig {
            session_meta: SessionMeta {
                game_id: self.game_id.to_string(),
                build_hash: self.build_hash.to_string(),
                platform: godot::classes::Os::singleton().get_name().to_string(),
                tick_rate,
                rng_seed: self.rng_seed as u64,
                created_at: unix_seconds(),
            },
            full_hash_interval: self.full_hash_interval.max(0) as u32,
            snapshot: tickwise::SnapshotPolicy::Off,
            // FNV-1a over the walk, which is nobody else's algorithm.
            hash_algo_id: 0,
            input_format_id: self.input_format_id as u64,
            dump_interval: self.dump_interval.max(0) as u32,
        };

        match Recorder::create(&resolved, config) {
            Ok(recorder) => {
                self.recorder = Some(recorder);
                self.next_tick = 0;
                true
            }
            Err(err) => {
                self.fail(&format!("cannot record to {resolved}: {err}"));
                false
            }
        }
    }

    /// Flushes and closes the recording. Doing this twice is harmless.
    #[func]
    pub fn stop_recording(&mut self) {
        let Some(recorder) = self.recorder.take() else {
            return;
        };
        if let Err(err) = recorder.finish() {
            self.last_error = err.to_string();
            godot_error!("tickwise: {}", self.last_error);
        }
    }

    /// Sets the input bytes for the current tick. Call it from a physics
    /// frame, before this node records. Tickwise never interprets these;
    /// they are whatever your netcode already sends.
    #[func]
    pub fn set_inputs(&mut self, inputs: PackedByteArray) {
        self.inputs.clear();
        self.inputs.extend_from_slice(inputs.as_slice());
    }

    /// Records a marker at the current tick, for example a round start.
    #[func]
    pub fn record_marker(&mut self, label: GString) {
        let tick = self.next_tick.saturating_sub(1);
        let label = label.to_string();
        let Some(recorder) = self.recorder.as_mut() else {
            return;
        };
        if let Err(err) = recorder.record_marker(tick, &label) {
            let message = err.to_string();
            self.fail(&message);
        }
    }

    /// True while ticks are being recorded.
    #[func]
    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// The number of ticks recorded so far, which is also the tick the
    /// next physics frame will use.
    #[func]
    pub fn get_tick(&self) -> i64 {
        self.next_tick as i64
    }

    /// The last failure, or an empty string. Recording stops at the
    /// first failure rather than repeating it sixty times a second, and
    /// the game keeps running.
    #[func]
    pub fn get_last_error(&self) -> GString {
        GString::from(self.last_error.as_str())
    }

    /// The number of nodes currently in scope. Zero means nothing is in
    /// the group, so every hash is the hash of an empty walk and no
    /// desync can ever be found.
    #[func]
    pub fn get_covered_node_count(&self) -> i64 {
        let Some(tree) = self.base().get_tree_or_null() else {
            return Default::default();
        };
        GodotProbe::collect(&tree, &self.group, &self.light_group).len() as i64
    }

    /// The full hash of the state right now, without recording it.
    ///
    /// Useful for a live check across the network: exchange the value
    /// every few seconds and you know the moment two clients disagree,
    /// long before the recordings are compared offline.
    #[func]
    pub fn state_hash(&self) -> i64 {
        let Some(tree) = self.base().get_tree_or_null() else {
            return Default::default();
        };
        GodotProbe::collect(&tree, &self.group, &self.light_group).full_hash() as i64
    }

    /// The state of every covered node as a dictionary of path and
    /// value, for logging or for an in-game inspector.
    #[func]
    pub fn state_snapshot(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some(tree) = self.base().get_tree_or_null() else {
            return Default::default();
        };
        let dump = GodotProbe::collect(&tree, &self.group, &self.light_group).state_dump();
        for (path, value) in dump.iter() {
            out.set(&GString::from(path), &value_to_variant(value));
        }
        out
    }

    fn fail(&mut self, message: &str) {
        self.last_error = message.to_string();
        godot_error!("tickwise: {message}");
        // The file is already unusable, and continuing would only
        // produce the same error on every frame.
        self.recorder = None;
    }
}

/// Renders a recorded value back into a Godot value, for inspection.
fn value_to_variant(value: &tickwise::Value) -> Variant {
    use tickwise::Value;
    match value {
        Value::Null => Variant::nil(),
        Value::Bool(v) => v.to_variant(),
        Value::I64(v) => v.to_variant(),
        Value::U64(v) | Value::Len(v) => (*v as i64).to_variant(),
        Value::F32(v) => f64::from(*v).to_variant(),
        Value::F64(v) => v.to_variant(),
        Value::Str(v) => GString::from(v.as_str()).to_variant(),
        Value::Bytes(v) => PackedByteArray::from(v.as_slice()).to_variant(),
    }
}

/// Wall clock seconds, recorded as metadata only. Time never reaches a
/// hash, and `compare` never looks at this field.
fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}
