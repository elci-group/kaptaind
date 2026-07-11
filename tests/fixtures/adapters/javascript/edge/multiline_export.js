// §8 hard case: `export` on its own line, declaration on the next.
// The adapter matches only `export ` (with trailing space) on a single line,
// so this is MISSED entirely.
export
function splitAcrossLines(x) {
  return x;
}
