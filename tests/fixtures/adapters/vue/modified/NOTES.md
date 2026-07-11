# Vue adapter — `modified` fixture pairs

Adapter: `src/diff/lang/adapters/vue.rs`
Detected extension: `.vue` (`detect_files` filters `extension() == "vue"`).

## Adapter mechanics relevant to `modified`

Only lines inside `<script ...>` / `</script>` are scanned (`in_script` gate).
Kinds are emitted by a single `if / else if` chain per line:

- line contains `defineProps`  -> `kind = "props"`,  `name = <entire trimmed line>`
- line contains `defineEmits`  -> `kind = "emits"`,  `name = <entire trimmed line>`
- line contains `defineExpose` -> `kind = "expose"`, `name = <entire trimmed line>`
- else if line starts with `export ` -> `kind = "export"`, `name = <text after "export ">`
  (this branch is unreachable when the line contains any macro keyword above)

Kind strings relied on (copied from source): `"props"`, `"emits"`, `"expose"`, `"export"`.

## IMPORTANT — same-name/different-kind is unreachable for this adapter

The `modified` signal needs same `name` + different `kind`. For the three macro
kinds the `name` is the WHOLE trimmed line, so flipping the kind necessarily
changes the keyword inside that very line, which changes the `name`. Two
text-identical lines always classify to the same kind (deterministic chain).
The `export` name can never equal a macro name either: a macro name always
contains a macro keyword, whereas the `export` branch is only reachable for
lines containing none of them. So no pair can be emitted as same-name/
different-kind. The kind-change pairs below isolate the kind flip to ONLY the
kind-bearing token (pair 1) or the smallest possible declaration change
(pairs 2-3), but they are expected to surface as `removed + added`, NOT
`modified`. They are included as real kind transitions the adapter does
distinguish, and to document the gap. Only the `control` pair is a true
no-`modified` guarantee.

## Pairs

### 1. props_to_emits
- NAME held constant (intended): the declaration line; ONLY the macro keyword
  is swapped (`defineProps` -> `defineEmits`), array form `['value']` kept
  identical because it is valid syntax for both macros.
- old_kind -> new_kind: `props` -> `emits`
- Breaking-policy hint: **depends** — a prop (consumer-supplied input) becoming
  an emit (component-emitted event) reverses the data-flow contract, which is
  breaking if a parent bound it as a prop, but a no-op if it was unused.
- Kind strings used: `"props"`, `"emits"`.
- Uncertainty: HIGH this is NOT emitted as `modified`. The name is the whole
  line, so changing `defineProps` to `defineEmits` changes the name too; the
  diff engine should report one removed `props` + one added `emits`.

### 2. emits_to_expose
- NAME held constant (intended): the binding `const api =`; macro keyword and
  the required payload shape change (`defineEmits(['change'])` takes an array;
  `defineExpose({ ... })` takes an object — kept valid per macro).
- old_kind -> new_kind: `emits` -> `expose`
- Breaking-policy hint: **depends** — emit (event contract) vs expose
  (template-ref/imperative handle contract) are different consumer surfaces;
  breaking for code listening to the event, irrelevant for template refs.
- Kind strings used: `"emits"`, `"expose"`.
- Uncertainty: HIGH this is NOT emitted as `modified` (whole-line name changes
  with the keyword and payload); expect removed `emits` + added `expose`.

### 3. expose_to_export
- NAME held constant (intended): identifier `api` and object literal
  `{ focus: () => {} }`; declaration style changes from compiler macro
  `defineExpose({...})` to a named `export const api = {...}`.
- old_kind -> new_kind: `expose` -> `export`
- Breaking-policy hint: **yes** — `defineExpose` controls the public
  template-ref surface of the component; replacing it with a module `export`
  removes that runtime handle, breaking parents using `ref.value.focus()`.
- Kind strings used: `"expose"`, `"export"`.
- Uncertainty: HIGH this is NOT emitted as `modified` (macro whole-line name
  vs `export` rest-of-line name are disjoint; different text, different name);
  expect removed `expose` + added `export`.

### 4. control
- NAME held constant: `const props = defineProps<{ label: string }>()` (the
  entire macro line, byte-identical across the pair).
- same_kind (control): `props` -> `props` (only template text
  `{{ label }}` -> `{{ label }}!` changed; the scanned script line is
  unchanged).
- Breaking-policy hint: **no** — no declaration changed; only template markup.
- Kind strings used: `"props"`.
- Uncertainty: LOW — this pair genuinely yields identical symbols and MUST
  produce no `modified` entry; it guards against over-firing.
