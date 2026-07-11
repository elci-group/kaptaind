// Retry policy expressed as a discriminated union (before).
type MaxRetries =
    | Default
    | Custom of int
