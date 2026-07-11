open List
include Stdlib
exception Empty
external input : in_channel -> bytes -> int -> int -> int = "caml_ml_input"
[@@@ocaml.warning "-32"]
let () = print_endline "boot"
let _ = 1 + 1
