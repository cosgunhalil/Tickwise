extends Node

# Every var declared here is a script variable, which is exactly what
# Tickwise walks. Integers only, so two runs agree bit for bit.
var pos_x: int = 0
var pos_y: int = 0
var vel_x: int = 0
var vel_y: int = 0

# A Vector2 as well, to prove composite values decompose into .x and .y
# leaves rather than being skipped.
var last_bounce: Vector2 = Vector2.ZERO
