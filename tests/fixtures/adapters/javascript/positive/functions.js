// Public named function exports — adapter should flag kind "function".
export function greet(name) {
  return `hi ${name}`;
}

export async function loadUser(id) {
  return fetch(`/u/${id}`);
}
