// String data and function calls that merely *look* like declarations.
// No line begins with a declaration keyword, so nothing is public API.
open System

printfn "module FakeModule"
printfn "let notARealBinding = 1"
printfn "type NotAType = | A | B"
System.Console.WriteLine("val notASignature : int")
