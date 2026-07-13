const std = @import("std");

fn main() void {
    const x: i32 = 3;
    var y: i32 = x + 1;
    y = greet(x);
    std.debug.print("{d}\n", .{y});
}

fn greet(v: i32) i32 {
    return v * 2;
}

test "math works" {
    const z = greet(2);
    _ = z;
}
