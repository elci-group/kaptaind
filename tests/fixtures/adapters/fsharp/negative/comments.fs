// A whole-line comment: let shouldBeIgnored = 1
// type AlsoIgnored = | A | B

(* single-line block comment: let hidden = 1 *)
(* type BlockHidden = { X : int } *)

// Block comment whose body stays on the opening line:
(* val notReal : int *)

open System
open Microsoft.FSharp.Core
open System.Text.RegularExpressions
