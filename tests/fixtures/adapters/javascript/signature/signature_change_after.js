// After: param added -> name changes -> old name removed -> breaking=true.
export function connect(host, port) {
  return open(host, port);
}
