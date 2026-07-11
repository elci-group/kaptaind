module Values

// Plain value
let answer = 42

// Recursive function -> 'rec' is a stripped modifier
let rec factorial n =
    if n <= 1 then 1 else n * factorial (n - 1)

// Inline function -> 'inline' is a stripped modifier
let inline square x = x * x

// Mutable binding -> 'mutable' is a stripped modifier
let mutable counter = 0

// Literal value (attribute stripped before keyword)
[<Literal>]
let Pi = 3.14159
