[<RequireQualifiedAccess>]
module Attributed

// Attribute on the same line as the keyword is stripped, name still captured
[<Obsolete("Use V2")>]
type Legacy = { Id : int }

// Nested brackets inside an attribute are handled by find_attr_end
[<Struct; CompiledName("Point")>]
type Point = { X : float; Y : float }

// Attribute + modifiers together
[<CompiledName("Compute")>]
let inline compute x = x + 1

[<Literal>]
let Version = "1.0"
