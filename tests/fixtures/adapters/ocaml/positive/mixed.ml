(* A small public surface *)
let version = "1.0.0"
let rec length = function
  | [] -> 0
  | _ :: tl -> 1 + length tl
type t = { label : string; value : int }
module Config = struct end
