pub fn greet(name: []const u8) void {
    _ = name;
}

export fn cEntry(x: c_int) c_int {
    return x;
}

pub extern "c" fn imported(x: c_int) c_int;

pub fn create(
    allocator: std.mem.Allocator,
    size: usize,
) !Foo {
    _ = allocator;
    _ = size;
}
