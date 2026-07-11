module Generics

// Generic type: take_identifier stops at '<', base name 'Box' is captured.
type Box<'a> = { Value : 'a }

// Generic value: same rule, base name 'identity' is captured.
let identity<'a> (x : 'a) = x

let map<'a, 'b> f (xs : 'a list) = List.map f xs
