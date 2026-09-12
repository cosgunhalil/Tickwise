extends Node

# This node is deliberately in no Tickwise group, for two reasons.
#
# Its noise value changes every tick and must never reach a hash, which
# is what proves coverage is a decision rather than a guess.
#
# It also holds the chaos switch. A switch on a grouped node would be
# part of the recorded state, and the two runs would then differ from
# tick 0 rather than from the tick the planted bug strikes. That is the
# probe being right and the test being wrong, so the switch lives here.
var noise: int = 0
var chaos_enabled: bool = false


func _physics_process(_delta: float) -> void:
	noise += 1
