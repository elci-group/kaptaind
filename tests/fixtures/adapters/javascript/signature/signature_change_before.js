// True breaking per source: a signature change alters the symbol `name`,
// so the old name is treated as removed (diff is name-based, no arity model).
export function connect(host) {
  return open(host);
}
