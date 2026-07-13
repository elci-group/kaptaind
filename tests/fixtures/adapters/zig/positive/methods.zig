pub const Stack = struct {
    items: [16]i32,
    len: usize,

    pub fn push(self: *Stack, value: i32) void {
        _ = self;
        _ = value;
    }

    pub fn pop(self: *Stack) i32 {
        _ = self;
        return 0;
    }

    fn grow(self: *Stack) void {
        _ = self;
    }
};
