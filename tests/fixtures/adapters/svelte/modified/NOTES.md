# svelte `modified` fixtures — kind-change before/after pairs

How the svelte adapter builds a symbol (`src/diff/lang/adapters/svelte.rs`):

- Only lines inside `<script>...</script>` are scanned.
- `export let <rest>`     → `kind = "prop"`,        `name = <rest>` (substring after the prefix).
- `export const <rest>`   → `kind = "export"`,      `name = <rest>` (substring after the prefix).
- `export function <rest>`→ `kind = "export"`,      `name = <rest>` (substring after the prefix).
- `$props(` / `$state(` / `$derived(` → `kind = "rune_props" / "rune_state" / "rune_derived"`,
  `name = <the entire trimmed line>`.

`modified` = same `name`, different `kind`. Because for the `export *` family `name` is the
substring AFTER the matched prefix, a pair holds the name constant by keeping that suffix byte-
identical while swapping the prefix keyword (`let` ↔ `const`), which flips the kind.

File extension used: `.svelte` (from `detect_files`, which filters `extension == "svelte"`).

---

## Pair 1 — `prop_to_export`

- NAME held constant: `count = 0;`
- `old_kind -> new_kind`: `prop -> export`
  - before: `export let count = 0;`   → name `count = 0;`, kind `"prop"`
  - after:  `export const count = 0;` → name `count = 0;`, kind `"export"`
- BREAKING-POLICY HINT: **yes** — `export let` declares a public component prop a parent can pass
  and `bind:`; `export const` is not a prop (only an instance export via `bind:this`), so
  `<Child count={...}/>` / `bind:count` consumers break.
- kind strings relied on: `"prop"` (svelte.rs `export let` branch), `"export"` (svelte.rs `export const` branch).
- UNCERTAINTY: low. Suffix `count = 0;` is byte-identical by construction and each line matches
  exactly one prefix in the `else if` chain. Cannot run the parser to confirm.

## Pair 2 — `export_to_prop`

- NAME held constant: `label = 'pending';`
- `old_kind -> new_kind`: `export -> prop` (reverse direction of Pair 1)
  - before: `export const label = 'pending';` → name `label = 'pending';`, kind `"export"`
  - after:  `export let label = 'pending';`   → name `label = 'pending';`, kind `"prop"`
- BREAKING-POLICY HINT: **depends** — widening a constant instance export into a reactive prop makes
  it parent-overridable and `bind:`-able; consumers relying on a stable constant may change behavior,
  though it is partly additive.
- kind strings relied on: `"export"` (svelte.rs `export const` branch), `"prop"` (svelte.rs `export let` branch).
- UNCERTAINTY: low. Same byte-identical-suffix reasoning as Pair 1, direction reversed.

## Pair 3 — `array_prop_to_export`

- NAME held constant: `items = [];`
- `old_kind -> new_kind`: `prop -> export`
  - before: `export let items = [];`   → name `items = [];`, kind `"prop"`
  - after:  `export const items = [];` → name `items = [];`, kind `"export"`
- BREAKING-POLICY HINT: **yes** — `items` stops being a prop, so a parent can no longer pass or bind
  the list; only instance-level access remains.
- kind strings relied on: `"prop"` (svelte.rs `export let` branch), `"export"` (svelte.rs `export const` branch).
- UNCERTAINTY: low. Array-literal suffix `items = [];` is byte-identical; valid for both `let` and
  `const`. Same prefix-matching reasoning.

## Pair 4 — `control` (negative control)

- NAME held constant: `count = 0;`
- `old_kind -> new_kind`: `same_kind (control)` — `prop -> prop`
  - before: `export let count = 0;` + `<p>before</p>`
  - after:  `export let count = 0;` + `<p>after</p>`
  - The declaration line is byte-identical; only the template markup outside `<script>` changed,
    which the parser ignores entirely.
- BREAKING-POLICY HINT: **no** — the component's prop/export contract is unchanged; only rendered
  text differs, so consumers are unaffected.
- kind strings relied on: `"prop"` (both sides).
- UNCERTAINTY: low that this yields NO modified symbol — name and kind are identical, so it is
  "unchanged" (no added/removed/modified). Cannot run the parser to confirm zero signals.

---

## Cross-cutting uncertainty / adapter limitation (read this)

The task asks for 3 pairs that "ideally" exercise DIFFERENT kind-transitions. This adapter's design
makes `prop <-> export` (`export let` <-> `export const`) the ONLY syntactically-valid same-name /
different-kind transition I can construct:

- `export const` and `export function` collapse to the SAME kind string `"export"`, so
  `const <-> function` is NOT a kind change (would not be `modified`).
- `let <-> function` and `const <-> function` cannot share one byte-identical suffix that is valid
  under both keywords (`function` needs `name(){...}`, `let`/`const` need `name = init`), so they
  cannot hold `name` constant while staying syntactically valid.
- Rune symbols (`rune_props`/`rune_state`/`rune_derived`) use the WHOLE trimmed line as `name`, so
  any token change changes the name — no rune pair can be same-name/different-kind.

Therefore all three kind-change pairs use the single available `prop <-> export` transition, varied
by held-name/suffix shape (number, string, array) and direction (Pair 2 is the reverse). This is a
structural limitation of the adapter, not a doubt about emission; per-pair emission confidence is high,
but I cannot run the parser to verify.
