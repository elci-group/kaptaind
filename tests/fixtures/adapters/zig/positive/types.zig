pub const Point = struct {
    x: f32,
    y: f32 = 0,
};

pub const Bits = packed struct {
    a: u1,
    b: u7,
};

pub const Color = enum {
    red,
    green,
};

pub const Value = union(enum) {
    i: i32,
    f: f32,
};

pub const Handle = opaque {};

pub const VERSION = "1.0.0";

pub var counter: u32 = 0;
