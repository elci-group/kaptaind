// Export-looking text inside strings must NOT be flagged,
// because none of these lines begin with `export ` / `module.exports`.
const msg = "export function fakeExport() {}";
const tpl = `export const notReal = 42`;
const code = "module.exports = function nope() {}";

function logger() {
  return "export default function pretend() {}";
}
