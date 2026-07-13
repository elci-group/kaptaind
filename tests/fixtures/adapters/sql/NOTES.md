# SQL adapter calibration corpus (adapter-200 item 10, rev 30)

Semantics: `CREATE <object> <name>` and `DROP <object> <name>` are schema API
surface; DML (`SELECT`/`INSERT`/`UPDATE`/`DELETE`) is not. SQL has no visibility
model — every schema object is public by definition (confidence band 0.7, the
no-visibility tier). `DROP` emits a distinct `drop_<object>` kind so a drop
registers as its own API event.

- positive/: schema objects across tables, views, indexes, routines, triggers,
  sequences, schemas, and drop-containing migrations → all must yield symbols.
- negative/: pure DML and comment-only files (incl. commented-out DDL) → zero
  symbols.
- breaking/: `drop_table`/`shrink_view` pairs remove a CREATE object →
  `diff.removed` non-empty → breaking fires. `control` adds a column inside an
  existing CREATE TABLE — symbol set unchanged → NOT breaking (by design;
  column-level shape is out of scope for the line scanner).
- modified/: same-name object changes kind (table→view, view→function,
  function→table) → X2 `modified` fires. `control` adds a column → no kind
  change → not modified (by design).
- signature/: absent — the adapter records no signatures (DDL arity is not
  modeled), so the harness reports 0/0 like other signature-less adapters.
