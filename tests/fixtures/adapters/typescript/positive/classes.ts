export class UserService {
    constructor(private readonly db: unknown) {}

    find(id: string): string {
        return id;
    }
}

export class Repository<T> {
    save(_value: T): void {}
}
