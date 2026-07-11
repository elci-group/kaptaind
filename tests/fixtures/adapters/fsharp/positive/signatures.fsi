namespace Sample

module Api =

    // Signature-only declarations; 'val' maps to kind "value"
    val add : int -> int -> int

    val isValid : string -> bool

    // A public type abbreviation exposed in the signature
    type Handler = string -> int
