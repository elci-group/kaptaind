type ('a, 'b) result = Ok of 'a | Error of 'b
let id (type a) (x : a) = x
module type OrderedType = sig type t val compare : t -> t -> int end
module Make (X : OrderedType) = struct end
