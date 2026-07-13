# HCL (Terraform) adapter calibration corpus (adapter-200 item 10, rev 31)

Semantics: labeled blocks are API surface — `variable`/`output` (the module
input/output contract), `resource`/`data` (managed infrastructure, emitted as
the qualified Terraform address `type.name` used by `terraform state` and
`moved` blocks), `module`/`provider` (composition). Unlabeled blocks
(`terraform`, `locals`, `moved`, `import`, `check`, `removed`) are structural
and not surface. `.tfvars` files are value assignments, not surface, and are
excluded by `detect_files`. HCL has no visibility model — every labeled block
is public by definition (confidence band 0.7, the no-visibility tier).

- positive/: labeled blocks across variables, outputs, resources, data
  sources, modules, providers, plus a `.hcl`-extension file → all must yield
  symbols.
- negative/: unlabeled blocks and attribute-only files, plus fake blocks in
  `#`/`//`/`/* */` comments and inside a `<<-EOF` heredoc body → zero
  symbols.
- breaking/: `remove_resource`/`remove_variable` pairs delete a labeled block
  → `diff.removed` non-empty → breaking fires. `control` adds an attribute
  inside an existing resource — symbol set unchanged → NOT breaking (by
  design; block-internal shape is out of scope for the line scanner).
- modified/: same-name block changes kind (variable→output, resource→data on
  the same qualified address, output→variable) → X2 `modified` fires.
  `control` adds an attribute → no kind change → not modified (by design).
- signature/: absent — the adapter records no signatures (HCL blocks have no
  parameter lists), so the harness reports 0/0 like other signature-less
  adapters.
