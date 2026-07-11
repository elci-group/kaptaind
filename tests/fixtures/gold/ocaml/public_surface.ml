module Config = struct
  let debug = false
end

module type STORE = sig
  val save : string -> unit
end

type point = { x : int; y : int }

let add x y = x + y
