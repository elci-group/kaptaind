module Types

// Record type
type Person =
    { Name : string
      Age : int }

// Generic discriminated union (base name only is captured)
type Result<'a, 'e> =
    | Ok of 'a
    | Error of 'e

// Single-case union wrapping a value
type UserId = UserId of int

// Class-style type; members (member ...) are not flagged by the adapter.
// (A nested 'let' inside the body WOULD be over-reported; see known misses.)
type Counter(start) =
    member this.Inc() = ()
    member this.Value = start
