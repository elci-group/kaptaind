export function add(a: number, b: number): number {
  return a + b;
}

export class Router {
  handle(): void {}
}

export interface Repo {
  save(): void;
}

export type Point = { x: number; y: number };

export enum Color {
  Red,
}

export const VERSION: string = "1.0";
