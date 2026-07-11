module Foo = struct end
module type S = sig val x : int end
module Make (X : S) = struct end
