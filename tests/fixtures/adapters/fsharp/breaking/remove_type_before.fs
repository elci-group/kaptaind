module Public

type Removed = { Id : int }
type Kept = { Name : string }

let toString (k : Kept) = k.Name
