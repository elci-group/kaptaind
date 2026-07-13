pub fn distance(a: Point, b: Point) f32 {
    _ = a;
    _ = b;
    return 0;
}

pub fn write(buf: []const u8, comptime T: type, cb: fn (i32, u8) void) void {
    _ = buf;
    _ = T;
    _ = cb;
}

pub fn maybeGet(key: []const u8) ?Value {
    _ = key;
    return null;
}
