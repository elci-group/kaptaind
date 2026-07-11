// §8 Re-exports / barrels: adapter counts every `export ...` line as a new
// public symbol of kind "export" (pass-through re-exports are NOT elided).
export * from "./utils";
export { foo, bar } from "./helpers";
export { default as Baz } from "./baz";
