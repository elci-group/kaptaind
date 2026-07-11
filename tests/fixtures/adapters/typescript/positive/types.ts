export interface Config {
    host: string;
    port: number;
}

export type ID = string;

export enum Status {
    Ok,
    Err,
}

export type Result<T> = { ok: T };
