# htmlcss `modified` fixture notes

Adapter: `src/diff/lang/adapters/htmlcss.rs` (read-only, STEP 1).

## CRITICAL FINDING — read this first

The shared `modified` signal (same `name`, different `kind`) is **structurally
unreachable** for this adapter. Evidence from the source:

- `name` is the **entire trimmed source line**: `name: line.to_string()`
  (htmlcss.rs:33 and :38). It is NOT a parsed identifier/token.
- `kind` is a pure, deterministic function of that same line, keyed on
  mutually exclusive prefixes:
  - `line.starts_with("--") && line.contains(':')` -> `kind: "css_var"` (htmlcss.rs:31-35)
  - `line.starts_with('.') && line.contains('{')`  -> `kind: "css_class"` (htmlcss.rs:36-40)

Because a line has exactly one leading prefix, it can never be both `css_var`
and `css_class`, and because `kind = f(line)` and `name = line`, an identical
`name` always implies an identical `kind`. There is no input for which this
adapter emits the same `name` with two different `kind`s. The engine therefore
cannot report `modified` for htmlcss; any kind flip necessarily changes the
line text and so surfaces as **remove(old) + add(new)**, not `modified`.

The three transition pairs below are the nearest realistic kind transitions,
holding the **human identifier token** (`brand`/`theme`/`accent`) constant even
though the adapter's `name` (the whole line) cannot be held constant. They are
included for review/policy, but an assertion harness MUST expect `added` +
`removed` for them, NOT `modified`. Only the `control` pair is a true
no-`modified` case.

Recommendation to the aggregator: either (a) exclude htmlcss from the
`modified` assertion set, or (b) fix the adapter to use a token-based `name`
(e.g. the selector text / var name without braces/values) before this signal
can be meaningfully tested. (Fixing the adapter is out of scope here and would
touch `src/**`, which this task forbids.)

File extension used: `.css` (adapter `detect_files` accepts `html`/`css`;
`.css` mirrors the proven-positive fixtures `classes.css` / `vars.css`).

Kind strings relied on (copied verbatim from htmlcss.rs):
- `"css_var"`   (htmlcss.rs:34)
- `"css_class"` (htmlcss.rs:39)

## Per-pair detail

| Pair | NAME held constant | old_kind -> new_kind | Expected engine signal | Breaking-policy hint |
|------|--------------------|----------------------|------------------------|----------------------|
| var_to_class_brand | token `brand` only (whole-line `name` differs: `--brand: #3366ff;` vs `.brand {`) | `css_var` -> `css_class` | added+removed, NOT modified | depends — a custom property and a class are different consumption channels (`var(--brand)` vs `.brand`); switching which one exists breaks whichever consumers used the old form, but keeping the token value is non-breaking in intent |
| class_to_var_theme | token `theme` only (`.theme {` vs `--theme: #111111;`) | `css_class` -> `css_var` | added+removed, NOT modified | depends — inverse of above; consumers selecting `.theme` break, consumers of `var(--theme)` are introduced; net effect depends on usage |
| var_to_class_accent | token `accent` only (`--accent: #ff6600;` vs `.accent {`) | `css_var` -> `css_class` | added+removed, NOT modified | depends — same reasoning as brand; a value-bearing var vs a selector are not interchangeable for consumers |
| control | `.btn {` (byte-identical after trim) | same_kind (control): `css_class` -> `css_class` | none (no added/removed/modified) | no — only a non-symbol comment line was added; declaration, name, and kind are unchanged |

Notes:
- All four pairs are syntactically valid CSS and mirror the structure of the
  proven-positive fixtures (`positive/classes.css`, `positive/vars.css`).
- The control after-file adds only `/* keep */` (starts with `/`, matches
  neither `css_var` nor `css_class` branch) so the parsed symbol set is
  identical and must produce NO `modified`, guarding against over-firing.

## Uncertainty

- Certainty is HIGH (not merely uncertainty) that the 3 transition pairs emit
  `added`+`removed` rather than `modified`, because `name` is the full trimmed
  line and the line text necessarily changes across a kind flip. This is
  determined by reading the adapter, not by running it (cargo/formatter/git were
  not run, per task rules).
- Minor residual uncertainty: whether the surrounding diff harness keys pairs
  strictly by `<case>_before/_after` and asserts exactly one `modified`. If so,
  the 3 transition pairs here will FAIL that assertion by design of the
  adapter; treat them as `breaking/`-style add+remove evidence instead.
- Control pair: high confidence it yields no `modified` (identical symbol set).
