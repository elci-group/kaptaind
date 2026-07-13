const std = @import("std");

pub const VERSION = "0.1.0";

pub const Point = struct {
    x: f32,
    y: f32,

    pub fn distance(a: Point, b: Point) f32 {
        _ = a;
        _ = b;
        return 0;
    }

    fn reset(self: *Point) void {
        _ = self;
    }
};

pub const Color = enum {
    red,
    green,
};

pub fn greet(name: []const u8) void {
    _ = name;
}

pub var counter: u32 = 0;

fn helper() void {}
