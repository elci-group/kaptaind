// pub fn fakeOne() void {}
/// pub fn fakeTwo() void {}
//! pub const FakeThree = struct {};
const text =
    \\pub fn fakeFour() void {}
    \\pub const FakeFive = enum { a };
;
pub const Real = struct {
    pub fn genuine() void {}
};
