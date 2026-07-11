// Script / preprocessor directives only. Every content line starts with '#'
// and is skipped by the adapter, so there are no public symbols here.
#r "nuget: FSharp.Data"
#load "helpers.fsx"
#if DEBUG
#else
#endif
#time
#nowarn "62"
