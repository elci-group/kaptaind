// Only private / internal declarations live here.
// No public module / type / let / val should be reported.

let private secret = 1
let private hiddenFn () = ()
let internal internalValue = "x"

type internal InternalType = class end
type private PrivateUnion = | A | B

module private PrivateModule = let x = 1
