extends SceneTree

# Headless driver. Builds a small deterministic simulation, records a
# fixed number of physics ticks, checks what the probe covers, and quits.
#
#   godot --headless --path godot --script res://main.gd -- \
#         --out <file> [--chaos]
#
# Exit code 0 means the recording was written and every assertion held.

const TICKS := 600
const BALLS := 4

var world = null
var recorder = null
var untracked = null
var out_path := ""
var frames := 0
var failures := 0


func fail(message: String) -> void:
	failures += 1
	printerr("FAIL: ", message)


func check(condition: bool, message: String) -> void:
	if not condition:
		fail(message)


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var chaos := args.has("--chaos")
	var index := args.find("--out")
	if index == -1 or index + 1 >= args.size():
		printerr("usage: --out <file> [--chaos]")
		quit(2)
		return
	out_path = args[index + 1]

	var game := Node.new()
	game.name = "Game"
	root.add_child(game)

	world = Node.new()
	world.name = "World"
	world.set_script(load("res://world.gd"))

	world.add_to_group("tickwise")
	world.add_to_group("tickwise_light")
	game.add_child(world)

	for i in BALLS:
		var ball := Node.new()
		ball.name = "Ball%d" % i
		ball.set_script(load("res://ball.gd"))
		ball.pos_x = 1000 + i * 700
		ball.pos_y = 2000 + i * 300
		ball.vel_x = 40 + i * 11
		ball.vel_y = 55 - i * 7
		ball.add_to_group("tickwise")
		ball.add_to_group("tickwise_light")
		game.add_child(ball)
		world.balls.append(ball)

	# In no group, so its every-tick change must stay out of the hash.
	untracked = Node.new()
	untracked.name = "Untracked"
	untracked.set_script(load("res://untracked.gd"))
	game.add_child(untracked)
	untracked.chaos_enabled = chaos
	world.config = untracked

	recorder = ClassDB.instantiate("TickwiseRecorder")
	if recorder == null:
		printerr("the Tickwise GDExtension did not load")
		quit(3)
		return
	recorder.name = "TickwiseRecorder"
	recorder.game_id = "tickwise-godot-tests"
	recorder.build_hash = "test-build"
	recorder.rng_seed = 12345
	recorder.full_hash_interval = 50
	recorder.input_format_id = 7
	game.add_child(recorder)
	world.recorder = recorder

	# A node outside the tree answers rather than crashing, which is what
	# a game gets if it asks before the scene is ready.
	var loose = ClassDB.instantiate("TickwiseRecorder")
	check(loose.state_hash() == 0, "a node outside the tree must not crash")
	check(loose.get_covered_node_count() == 0, "no coverage outside the tree")
	loose.free()

	check(not recorder.is_recording(), "not recording before start")
	if not recorder.start_recording(out_path):
		fail("start_recording: " + recorder.get_last_error())
		quit(1)
		return
	check(recorder.is_recording(), "recording after start")


func _physics_process(_delta: float) -> bool:
	# This callback runs before the node callbacks of the same frame, so
	# the recorder's own count is the one to trust, not a local counter.
	frames += 1
	if recorder.get_tick() < TICKS:
		return false

	check(recorder.get_tick() == TICKS, "recorded %d ticks, expected %d" % [recorder.get_tick(), TICKS])
	check(recorder.get_last_error() == "", "error during recording: " + recorder.get_last_error())
	recorder.stop_recording()
	check(not recorder.is_recording(), "stopped")

	check_coverage()

	if failures == 0:
		print("recorded %d ticks to %s" % [TICKS, out_path])
	quit(1 if failures > 0 else 0)
	return true


# What the probe sees, checked once the tree is live and the recording is
# closed, so none of this can disturb the recorded session.
func check_coverage() -> void:
	check(recorder.get_covered_node_count() == BALLS + 1, "covered node count")

	var before: int = recorder.state_hash()
	untracked.noise += 1000
	check(recorder.state_hash() == before, "an ungrouped node must not change the hash")
	world.score += 1
	check(recorder.state_hash() != before, "a grouped node must change the hash")
	world.score -= 1
	check(recorder.state_hash() == before, "and changing it back must restore the hash")

	var snapshot: Dictionary = recorder.state_snapshot()
	check(snapshot.has("/root/Game/World.score"), "the snapshot names nodes by path")
	check(snapshot.has("/root/Game/Ball0.pos_x"), "script variables are covered")
	check(snapshot.has("/root/Game/Ball0.last_bounce.x"), "composite values decompose")
	check(not snapshot.has("/root/Game/Untracked.noise"), "an ungrouped node is absent")
	check(not snapshot.has("/root/Game/Ball0.name"), "engine properties stay out")
	check(not snapshot.has("/root/Game/Ball0.position"), "engine properties stay out")
