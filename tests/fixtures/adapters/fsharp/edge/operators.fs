module Ops

// Custom operator: take_identifier breaks on '(', so '(++)' is NOT detected.
let (++) a b = a + b

// Active pattern: also starts with '(', so the cases are NOT detected.
let (|Even|Odd|) n = if n % 2 = 0 then Even else Odd

// A plain binding to confirm the file is otherwise parsed.
let normal = 1
