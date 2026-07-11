module Geometry

open System

// Public discriminated union
type Shape =
    | Circle of float
    | Rectangle of float * float

// Public function (indented inside module -> still parsed after trim_start)
let area shape =
    match shape with
    | Circle r -> Math.PI * r * r
    | Rectangle (w, h) -> w * h

let private helper x = x + 1
