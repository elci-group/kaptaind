module Signatures

// A value binding present in the script
let greeting name = sprintf "Hello, %s" name

// A nested module with a public member
module Inner =
    let double x = x * 2

// A public type used by the script
type Config = { Verbose : bool }

// open statements are ignored
open System.IO
