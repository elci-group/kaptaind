# TypeScript `modified` fixture notes

Signal under test: the diff engine flags a symbol `modified` when the NAME is
unchanged but the KIND changes (same name, different `kind`). Each pair below
holds the extracted name token byte-identical and swaps the kind-bearing keyword.

Adapter: `src/diff/lang/adapters/typescript.rs` (`TypeScriptAdapter`).
Detected extensions: `ts`, `tsx` (these fixtures use `.ts`).
Note: `parse_ast` delegates to `ts_parse` in `src/diff/lang/common.rs`, which is
outside this lane and was NOT read. The `kind` strings below are taken from the
adapter's own `#[cfg(test)]` assertions (`typescript_classifies_export_kinds`,
`typescript_detects_hooks_and_middleware`), not from the shared parser source.
Name extraction is inferred to be the identifier immediately following the
declaration keyword (`interface`/`type`/`const` ...), consistent with the names
asserted in those tests (`Config`, `ID`, `VERSION`).

## Pair 1 — `iface_to_type`
- NAME held constant: `Config`
- old_kind -> new_kind: `interface` -> `type`
- breaking-policy hint: yes — an interface is extendable/mergeable and
  structurally typed with declaration merging; a type alias is not augmentable
  and has different assignability edge cases, so consumers using `extends`,
  `implements`, or augmentation can break.
- kind strings relied on: `"interface"`, `"type"`
- uncertainty: low. Both kinds are directly asserted by the adapter tests and
  the name token (`Config`) is the identifier right after the keyword in both.

## Pair 2 — `iface_to_binding`
- NAME held constant: `Settings`
- old_kind -> new_kind: `interface` -> `binding`
- breaking-policy hint: yes — a type-only interface becomes a runtime value
  (const object); type-only imports, `implements`, and generic usage break,
  while value-space usage newly appears. Changes the namespace (type vs value).
- kind strings relied on: `"interface"`, `"binding"`
- uncertainty: low–medium. `interface` and `binding` (from `export const`) are
  both asserted. Confidence depends on the const-object initializer still being
  classified as `binding`; it mirrors the asserted `export const VERSION = 1`.

## Pair 3 — `type_to_binding`
- NAME held constant: `ID`
- old_kind -> new_kind: `type` -> `binding`
- breaking-policy hint: yes — a type alias (type-space only) becomes a runtime
  const (value-space); consumers using it as a type annotation lose the type,
  and value usage semantics change entirely.
- kind strings relied on: `"type"`, `"binding"`
- uncertainty: low. `export type ID = string` (asserted `type`) vs
  `export const ID = '...'` (const -> asserted `binding` shape). Both evidenced.

## Pair 4 — `control`
- NAME held constant: `Config`
- old_kind -> new_kind: same_kind (control) — `interface` -> `interface`
- breaking-policy hint: no — only a member was added to the interface body;
  name and kind are unchanged, so it must NOT be reported as `modified`
  (guards against over-firing). Adding an optional member is generally
  non-breaking for structural typing.
- kind strings relied on: `"interface"` (both sides)
- uncertainty: low. Same declaration shape as the asserted `interface` case;
  only the body changed.

## General uncertainty
- Could not read `common.rs` (out of lane) or run the parser, so kind strings
  are sourced solely from the adapter's in-file tests. Kinds not asserted there
  (e.g. a hypothetical `function`/`class` string) were deliberately avoided.
- Assumes name extraction uses the raw identifier token with no
  normalization/prefixing; if the parser qualifies names (e.g. includes
  `export`), the byte-identical assumption could fail for all pairs uniformly.
