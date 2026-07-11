// Module-private members: no "export " prefix — must NOT be public.
function internalHelper() {
  return 42;
}

const moduleState = { ok: true };
let counter = 0;

class LocalCache {
  get(k) {
    return k;
  }
}
