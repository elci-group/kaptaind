// Export-looking tokens inside comments must NOT be flagged.
// No line here begins with the literal token `export `.
// export function commentedOut() {}
// export const ALSO_NOT = 1;
/* export class HiddenInBlock {} */
/**
 * export function insideDocBlock() {}
 */
const real = "not an export";
