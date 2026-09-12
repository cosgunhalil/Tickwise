extends Node

# The simulation. It steps every ball in a fixed order from one physics
# callback, so the order never depends on how the engine happens to walk
# the tree.

const ARENA := 16000
const CHAOS_AT_TICK := 421

var score: int = 0
var rng_state: int = 12345
var tick: int = 0

# The chaos switch lives on an ungrouped node, because a switch here
# would be part of the recorded state and the runs would differ from
# tick 0 instead of from the tick the bug strikes.

var config = null

var balls: Array = []
var recorder = null


func next_random() -> int:
	rng_state = (rng_state * 1664525 + 1013904223) & 0xFFFFFFFF
	return rng_state


func _physics_process(_delta: float) -> void:
	# Scripted inputs, so two runs differ only by the planted bug.
	var input := (tick / 45) % 4
	if recorder != null:
		recorder.set_inputs(PackedByteArray([input]))

	var nudge_x := 8 if input & 1 else 0
	var nudge_y := 8 if input & 2 else 0

	var first := true
	for ball in balls:
		ball.vel_x = clampi(ball.vel_x + nudge_x, -300, 300)
		ball.vel_y = clampi(ball.vel_y + nudge_y, -300, 300)
		ball.pos_x += ball.vel_x
		ball.pos_y += ball.vel_y

		if ball.pos_x < 0 or ball.pos_x > ARENA:
			ball.vel_x = -ball.vel_x
			ball.pos_x = clampi(ball.pos_x, 0, ARENA)
			ball.last_bounce = Vector2(float(tick), 0.0)
			score += 1
		if ball.pos_y < 0 or ball.pos_y > ARENA:
			ball.vel_y = -ball.vel_y
			ball.pos_y = clampi(ball.pos_y, 0, ARENA)
			ball.last_bounce = Vector2(0.0, float(tick))
			score += 1

		# The planted bug, applied after the bounce so nothing can undo
		# it before the tick is recorded.
		if first and config != null and config.chaos_enabled and tick >= CHAOS_AT_TICK:
			ball.pos_x += 3
		first = false

	next_random()
	tick += 1
