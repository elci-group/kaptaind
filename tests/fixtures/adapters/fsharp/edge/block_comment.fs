module Commented

// The adapter only skips the line that *starts* a block comment ('(*').
// A declaration on its own line inside the block is still parsed.
(* this is a multi-line block comment
let ghost = 1
*)
let live = 2
