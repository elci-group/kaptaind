const payload = "export function fake() {}";
const tmpl = `export const alsoFake = 1;`;

// CommonJS / computed export surface: not seen by the line scanner.
module.exports = { foo: 1 };
exports.bar = function () {};
Object.assign(exports, { baz: 2 });
