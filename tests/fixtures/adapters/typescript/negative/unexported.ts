function internal(): number {
    return 1;
}

const hidden = 'nope';

class Service {
    private secret = 0;
    protected half = 1;
    #trulyPrivate = 2;

    public visible(): void {}
}
